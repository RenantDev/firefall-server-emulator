// Definicoes de pacotes do protocolo Matrix UDP
//
// Protocolo de handshake (4 passos, Big Endian):
//   1. Client -> Server: POKE (solicita conexao)
//   2. Server -> Client: HEHE (atribui socket ID)
//   3. Client -> Server: KISS (confirma protocolo)
//   4. Server -> Client: HUGG (confirma conexao, informa porta do game server)
//
// Formato de pacotes pos-handshake:
//   [uint32 SocketID] [uint16 Header]
//   Header bits: [15-14 Canal] [13-12 Resend] [11 Split] [10-0 PayloadLen]

use bytes::{Buf, BufMut, BytesMut};

// ==================== Constantes do Protocolo ====================

/// Versao do protocolo esperada pelo client (build 1962)
pub const PROTOCOL_VERSION: u32 = 0x0004B968; // 309608 decimal

/// Streaming protocol esperado no KISS
pub const STREAMING_PROTOCOL: u16 = 0x4C5F;

/// Magics de handshake (4 bytes ASCII)
pub const MAGIC_POKE: [u8; 4] = *b"POKE";
pub const MAGIC_HEHE: [u8; 4] = *b"HEHE";
pub const MAGIC_KISS: [u8; 4] = *b"KISS";
pub const MAGIC_HUGG: [u8; 4] = *b"HUGG";
pub const MAGIC_ABRT: [u8; 4] = *b"ABRT";

/// Tamanho minimo de um pacote de handshake (socket_id + magic)
pub const MIN_HANDSHAKE_SIZE: usize = 8;

// ==================== Tipos de Pacote ====================

/// Pacote recebido do client, ja parseado
#[derive(Debug)]
pub enum ClientPacket {
    /// Passo 1: Client solicita conexao
    Poke {
        protocol_version: u32,
    },
    /// Passo 3: Client confirma protocolo
    Kiss {
        socket_id: u32,
        protocol_version: u16,
        streaming_protocol: u16,
    },
    /// Abort: client quer desconectar
    Abort {
        socket_id: u32,
    },
    /// Pacote de dados pos-handshake
    Data {
        socket_id: u32,
        channel: u8,
        resend_count: u8,
        is_split: bool,
        payload: Vec<u8>,
    },
    /// Pacote desconhecido/malformado
    Unknown {
        raw: Vec<u8>,
    },
}

/// Pacote para enviar ao client
#[derive(Debug)]
pub enum ServerPacket {
    /// Passo 2: Servidor atribui socket ID
    Hehe {
        assigned_socket_id: u32,
    },
    /// Passo 4: Servidor confirma conexao
    Hugg {
        socket_id: u32,
        sequence_start: u16,
        game_server_port: u16,
    },
    /// Abort: servidor quer desconectar
    Abort {
        socket_id: u32,
    },
    /// Pacote de dados pos-handshake (enviado pelo servidor)
    Data {
        socket_id: u32,
        channel: u8,
        payload: Vec<u8>,
    },
}

// ==================== Parsing de Pacotes ====================

/// Parseia um buffer UDP recebido do client
pub fn parse_client_packet(data: &[u8]) -> ClientPacket {
    if data.len() < MIN_HANDSHAKE_SIZE {
        tracing::warn!(
            "Pacote muito pequeno ({} bytes), minimo esperado: {}",
            data.len(),
            MIN_HANDSHAKE_SIZE
        );
        return ClientPacket::Unknown {
            raw: data.to_vec(),
        };
    }

    let mut buf = &data[..];

    // Primeiro campo: uint32 SocketID (Big Endian)
    let socket_id = buf.get_u32();

    // Segundo campo: 4 bytes de magic
    let mut magic = [0u8; 4];
    magic.copy_from_slice(&buf[..4]);
    buf.advance(4);

    match &magic {
        b"POKE" => {
            // POKE: [uint32 SocketID=0] [char[4] "POKE"] [uint32 ProtocolVersion]
            if buf.remaining() < 4 {
                tracing::warn!("POKE recebido mas sem ProtocolVersion (faltam bytes)");
                return ClientPacket::Unknown {
                    raw: data.to_vec(),
                };
            }
            let protocol_version = buf.get_u32();
            tracing::info!(
                "POKE recebido: socket_id=0x{:08X}, protocol_version=0x{:08X} ({})",
                socket_id,
                protocol_version,
                protocol_version
            );
            ClientPacket::Poke { protocol_version }
        }

        b"KISS" => {
            // KISS: [uint32 0] [char[4] "KISS"] [uint32 AssignedSocketID] [uint16 StreamingProtocol]
            // O primeiro uint32 (socket_id) e sempre 0 no KISS.
            // O socket_id real (atribuido no HEHE) vem DENTRO do payload como uint32.
            if buf.remaining() < 6 {
                tracing::warn!("KISS recebido mas sem dados suficientes (faltam bytes, precisa 6)");
                return ClientPacket::Unknown {
                    raw: data.to_vec(),
                };
            }
            let assigned_socket_id = buf.get_u32(); // socket_id do HEHE
            let streaming_proto = buf.get_u16();
            tracing::info!(
                "KISS recebido: assigned_socket_id=0x{:08X}, streaming=0x{:04X}",
                assigned_socket_id,
                streaming_proto
            );
            ClientPacket::Kiss {
                socket_id: assigned_socket_id,
                protocol_version: 0, // nao enviado no KISS
                streaming_protocol: streaming_proto,
            }
        }

        b"ABRT" => {
            tracing::info!("ABRT recebido: socket_id=0x{:08X}", socket_id);
            ClientPacket::Abort { socket_id }
        }

        _ => {
            // Nao e handshake - tentar parsear como pacote de dados pos-handshake
            // Formato: [uint32 SocketID] [uint16 Header] [payload...]
            // O "magic" que lemos na verdade sao os primeiros bytes apos o socket_id
            // Precisamos re-interpretar: os 4 bytes de "magic" + restante = header + payload

            // Recompor o buffer a partir dos 4 bytes de "magic" + restante
            let header_and_payload = &data[4..]; // tudo apos o socket_id

            if header_and_payload.len() < 2 {
                tracing::trace!(
                    "Pacote desconhecido: socket_id=0x{:08X}, magic={:?}, tamanho={}",
                    socket_id,
                    magic,
                    data.len()
                );
                return ClientPacket::Unknown {
                    raw: data.to_vec(),
                };
            }

            let mut hbuf = &header_and_payload[..];
            let header = hbuf.get_u16();

            let channel = ((header >> 14) & 0x03) as u8;
            let resend_count = ((header >> 12) & 0x03) as u8;
            let is_split = ((header >> 11) & 0x01) == 1;
            let payload_len = (header & 0x07FF) as usize;

            let remaining = &header_and_payload[2..];

            // Validar tamanho do payload
            if remaining.len() < payload_len {
                tracing::warn!(
                    "Pacote de dados truncado: socket_id=0x{:08X}, canal={}, esperado={} bytes, recebido={}",
                    socket_id,
                    channel,
                    payload_len,
                    remaining.len()
                );
            }

            let actual_len = payload_len.min(remaining.len());
            let payload = remaining[..actual_len].to_vec();

            tracing::debug!(
                "Pacote de dados: socket_id=0x{:08X}, canal={}, resend={}, split={}, payload_len={}, header_raw=0x{:04X}",
                socket_id,
                channel,
                resend_count,
                is_split,
                actual_len,
                header
            );

            // Log hex dump dos primeiros bytes do payload para debug
            if !payload.is_empty() {
                let hex_preview: String = payload
                    .iter()
                    .take(64)
                    .map(|b| format!("{:02X}", b))
                    .collect::<Vec<_>>()
                    .join(" ");
                tracing::debug!("  payload hex: {}", hex_preview);
            }

            ClientPacket::Data {
                socket_id,
                channel,
                resend_count,
                is_split,
                payload,
            }
        }
    }
}

// ==================== Serializacao de Pacotes ====================

/// Serializa um pacote do servidor para bytes prontos para envio UDP
pub fn serialize_server_packet(packet: &ServerPacket) -> Vec<u8> {
    match packet {
        ServerPacket::Hehe { assigned_socket_id } => {
            // HEHE: [uint32 SocketID=0] [char[4] "HEHE"] [uint32 AssignedSocketID]
            let mut buf = BytesMut::with_capacity(12);
            buf.put_u32(0x00000000); // SocketID fixo em 0 para handshake
            buf.put_slice(&MAGIC_HEHE);
            buf.put_u32(*assigned_socket_id);

            tracing::info!(
                "HEHE enviado: assigned_socket_id=0x{:08X}",
                assigned_socket_id
            );
            let bytes = buf.to_vec();
            log_hex_dump("HEHE ->", &bytes);
            bytes
        }

        ServerPacket::Hugg {
            socket_id: _,
            sequence_start,
            game_server_port,
        } => {
            // HUGG: [uint32 0] [char[4] "HUGG"] [uint16 SequenceStart] [uint16 GameServerPort]
            // O primeiro uint32 e SEMPRE 0 durante handshake (como HEHE)
            // O client usa o socket_id atribuido no HEHE, nao precisa repetir aqui
            let mut buf = BytesMut::with_capacity(12);
            buf.put_u32(0x00000000); // sempre 0 no handshake
            buf.put_slice(&MAGIC_HUGG);
            buf.put_u16(*sequence_start);
            buf.put_u16(*game_server_port);

            tracing::info!(
                "HUGG enviado: seq_start={}, port={}",
                sequence_start,
                game_server_port
            );
            let bytes = buf.to_vec();
            log_hex_dump("HUGG ->", &bytes);
            bytes
        }

        ServerPacket::Abort { socket_id } => {
            // ABRT: [uint32 SocketID] [char[4] "ABRT"]
            let mut buf = BytesMut::with_capacity(8);
            buf.put_u32(*socket_id);
            buf.put_slice(&MAGIC_ABRT);

            tracing::info!("ABRT enviado: socket_id=0x{:08X}", socket_id);
            buf.to_vec()
        }

        ServerPacket::Data {
            socket_id,
            channel,
            payload,
        } => {
            // Pacote de dados: [uint32 SocketID] [uint16 Header] [payload...]
            // Header bits: [15-14 Canal] [13-12 Resend=0] [11 Split=0] [10-0 PayloadLen]
            let payload_len = payload.len().min(0x07FF) as u16; // max 2047 bytes
            let header: u16 = ((*channel as u16 & 0x03) << 14) | payload_len;

            let mut buf = BytesMut::with_capacity(6 + payload.len());
            buf.put_u32(*socket_id);
            buf.put_u16(header);
            buf.put_slice(payload);

            tracing::debug!(
                "Data enviado: socket=0x{:08X}, canal={}, header=0x{:04X}, payload={}B",
                socket_id,
                channel,
                header,
                payload.len()
            );
            let bytes = buf.to_vec();
            log_hex_dump("DATA ->", &bytes);
            bytes
        }
    }
}

// ==================== Utilidades ====================

/// Log hex dump de um buffer (para debug do protocolo)
fn log_hex_dump(label: &str, data: &[u8]) {
    let hex: String = data
        .iter()
        .map(|b| format!("{:02X}", b))
        .collect::<Vec<_>>()
        .join(" ");
    tracing::debug!("{} [{}B]: {}", label, data.len(), hex);
}

/// Log hex dump de dados recebidos do client (para debug)
pub fn log_received_hex(data: &[u8], addr: &std::net::SocketAddr) {
    let hex: String = data
        .iter()
        .take(128) // limitar a 128 bytes no log
        .map(|b| format!("{:02X}", b))
        .collect::<Vec<_>>()
        .join(" ");
    let suffix = if data.len() > 128 { "..." } else { "" };
    tracing::debug!(
        "<- {} [{}B]: {}{}",
        addr,
        data.len(),
        hex,
        suffix
    );
}

// ==================== Testes ====================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_poke() {
        // POKE: socket_id=0, magic="POKE", version=0x0004B968
        let data: Vec<u8> = vec![
            0x00, 0x00, 0x00, 0x00, // socket_id = 0
            0x50, 0x4F, 0x4B, 0x45, // "POKE"
            0x00, 0x04, 0xB9, 0x68, // protocol_version = 309608
        ];
        match parse_client_packet(&data) {
            ClientPacket::Poke { protocol_version } => {
                assert_eq!(protocol_version, PROTOCOL_VERSION);
            }
            other => panic!("Esperado Poke, recebido: {:?}", other),
        }
    }

    #[test]
    fn test_parse_kiss() {
        // KISS: [uint32 0] [char[4] "KISS"] [uint32 AssignedSocketID=0x48] [uint16 StreamingProto=0x4C5F]
        let data: Vec<u8> = vec![
            0x00, 0x00, 0x00, 0x00, // socket_id = 0 (sempre 0 no KISS)
            0x4B, 0x49, 0x53, 0x53, // "KISS"
            0x00, 0x00, 0x00, 0x48, // assigned_socket_id = 0x48
            0x4C, 0x5F,             // streaming_protocol = 0x4C5F
        ];
        match parse_client_packet(&data) {
            ClientPacket::Kiss {
                socket_id,
                protocol_version: _,
                streaming_protocol,
            } => {
                assert_eq!(socket_id, 0x48);
                assert_eq!(streaming_protocol, STREAMING_PROTOCOL);
            }
            other => panic!("Esperado Kiss, recebido: {:?}", other),
        }
    }

    #[test]
    fn test_parse_abrt() {
        let data: Vec<u8> = vec![
            0x00, 0x00, 0x00, 0x01, // socket_id = 1
            0x41, 0x42, 0x52, 0x54, // "ABRT"
        ];
        match parse_client_packet(&data) {
            ClientPacket::Abort { socket_id } => {
                assert_eq!(socket_id, 1);
            }
            other => panic!("Esperado Abort, recebido: {:?}", other),
        }
    }

    #[test]
    fn test_serialize_hehe() {
        let packet = ServerPacket::Hehe {
            assigned_socket_id: 0x12345678,
        };
        let bytes = serialize_server_packet(&packet);
        assert_eq!(bytes.len(), 12);
        assert_eq!(&bytes[0..4], &[0x00, 0x00, 0x00, 0x00]); // socket_id = 0
        assert_eq!(&bytes[4..8], b"HEHE");
        assert_eq!(&bytes[8..12], &[0x12, 0x34, 0x56, 0x78]); // assigned_socket_id
    }

    #[test]
    fn test_serialize_hugg() {
        let packet = ServerPacket::Hugg {
            socket_id: 0xAABBCCDD, // ignorado, sempre envia 0
            sequence_start: 1,
            game_server_port: 25001,
        };
        let bytes = serialize_server_packet(&packet);
        assert_eq!(bytes.len(), 12);
        assert_eq!(&bytes[0..4], &[0x00, 0x00, 0x00, 0x00]); // sempre 0 no handshake
        assert_eq!(&bytes[4..8], b"HUGG");
        assert_eq!(&bytes[8..10], &[0x00, 0x01]); // sequence_start = 1
        assert_eq!(u16::from_be_bytes([bytes[10], bytes[11]]), 25001); // port
    }

    #[test]
    fn test_parse_data_packet() {
        // Pacote de dados: socket_id=0x00000001, header com canal=1, payload de 3 bytes
        // Header: canal=1 (bits 15-14 = 01), resend=0, split=0, len=3
        // Header = 0b01_00_0_00000000011 = 0x4003
        let data: Vec<u8> = vec![
            0x00, 0x00, 0x00, 0x01, // socket_id = 1
            0x40, 0x03,             // header = 0x4003
            0xAA, 0xBB, 0xCC,      // payload (3 bytes)
        ];
        match parse_client_packet(&data) {
            ClientPacket::Data {
                socket_id,
                channel,
                resend_count,
                is_split,
                payload,
            } => {
                assert_eq!(socket_id, 1);
                assert_eq!(channel, 1);
                assert_eq!(resend_count, 0);
                assert_eq!(is_split, false);
                assert_eq!(payload, vec![0xAA, 0xBB, 0xCC]);
            }
            other => panic!("Esperado Data, recebido: {:?}", other),
        }
    }

    #[test]
    fn test_parse_too_small() {
        let data: Vec<u8> = vec![0x00, 0x01];
        match parse_client_packet(&data) {
            ClientPacket::Unknown { .. } => {} // ok
            other => panic!("Esperado Unknown, recebido: {:?}", other),
        }
    }

    #[test]
    fn test_serialize_data_packet() {
        let packet = ServerPacket::Data {
            socket_id: 0x00000001,
            channel: 0, // canal de controle
            payload: vec![0x02, 0x00, 0x01, 0x00, 0x00],
        };
        let bytes = serialize_server_packet(&packet);
        // [uint32 socket_id=1] [uint16 header] [payload]
        assert_eq!(&bytes[0..4], &[0x00, 0x00, 0x00, 0x01]);
        // Header: canal=0 (bits 15-14 = 00), resend=0, split=0, len=5
        // Header = 0b00_00_0_00000000101 = 0x0005
        assert_eq!(&bytes[4..6], &[0x00, 0x05]);
        assert_eq!(&bytes[6..], &[0x02, 0x00, 0x01, 0x00, 0x00]);
    }

    #[test]
    fn test_serialize_data_packet_channel1() {
        let packet = ServerPacket::Data {
            socket_id: 0xAABBCCDD,
            channel: 1, // canal reliable
            payload: vec![0x00, 0x00, 0x01, 0xDE, 0xAD],
        };
        let bytes = serialize_server_packet(&packet);
        assert_eq!(&bytes[0..4], &[0xAA, 0xBB, 0xCC, 0xDD]);
        // Header: canal=1 (bits 15-14 = 01), resend=0, split=0, len=5
        // Header = 0b01_00_0_00000000101 = 0x4005
        assert_eq!(&bytes[4..6], &[0x40, 0x05]);
        assert_eq!(&bytes[6..], &[0x00, 0x00, 0x01, 0xDE, 0xAD]);
    }
}

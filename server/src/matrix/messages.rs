// Definicoes de mensagens do protocolo Matrix
//
// Mensagens sao transportadas dentro de pacotes de dados, em canais especificos.
// Canal 0: controle (ACK, TimeSync, keepalive) - sem numero de sequencia
// Canal 1: reliable, mensagens abstratas (Login, WelcomeToTheMatrix, EnterZone)
//   Formato: [uint16 SeqNum] [uint8 MessageId] [message_data...]
// Canal 2: reliable + GSS header
// Canal 3: unreliable + GSS header
//
// Todas as mensagens usam Big Endian.

use bytes::{Buf, BufMut, BytesMut};

// ==================== IDs de Mensagem - Canal 0 (Controle) ====================
// Fonte: AeroMessages/Control/ e PIN/UdpHosts/GameServer/Enums/ControlPacketType.cs

/// Close connection (ID 0)
pub const CTRL_CLOSE_CONNECTION: u8 = 0;
/// ACK do Matrix server confirmando recebimento de pacote reliable (canal 1)
pub const CTRL_MATRIX_ACK: u8 = 2;
/// ACK do GSS (Game State Server) confirmando pacotes do canal 2
pub const CTRL_GSS_ACK: u8 = 3;
/// Time sync request do client
pub const CTRL_TIME_SYNC_REQUEST: u8 = 4;
/// Time sync response do servidor
pub const CTRL_TIME_SYNC_RESPONSE: u8 = 5;

// ==================== IDs de Mensagem - Canal 1 (Matrix Reliable) ====================
// Fonte: AeroMessages/Matrix/V25/ e PIN/UdpHosts/GameServer/Enums/MatrixPacketType.cs

/// Login do client (primeiro pacote reliable apos handshake)
pub const MSG_LOGIN: u8 = 17;
/// EnterZoneAck - client confirma entrada na zona
pub const MSG_ENTER_ZONE_ACK: u8 = 18;
/// ExitZoneAck - client confirma saida da zona
pub const MSG_EXIT_ZONE_ACK: u8 = 19;
/// KeyframeRequest - client pede keyframe
pub const MSG_KEYFRAME_REQUEST: u8 = 20;
/// ClientStatus - status periodico do client
pub const MSG_CLIENT_STATUS: u8 = 25;
/// ClientPreferences - preferencias do client
pub const MSG_CLIENT_PREFERENCES: u8 = 26;
/// SuperPing - ping do client
pub const MSG_SUPER_PING: u8 = 28;
/// WelcomeToTheMatrix - resposta do servidor ao login
pub const MSG_WELCOME_TO_THE_MATRIX: u8 = 35;
/// Announce - broadcast do servidor
pub const MSG_ANNOUNCE: u8 = 36;
/// EnterZone - servidor informa ao client para entrar numa zona
pub const MSG_ENTER_ZONE: u8 = 37;
/// UpdateZoneTimeSync - update de tempo da zona
pub const MSG_UPDATE_ZONE_TIME_SYNC: u8 = 38;
/// ExitZone - servidor manda client sair da zona
pub const MSG_EXIT_ZONE: u8 = 40;
/// MatrixStatus - status do servidor
pub const MSG_MATRIX_STATUS: u8 = 41;

// ==================== Mensagens de Controle (Canal 0) ====================

/// MatrixAck - servidor confirma recebimento de pacote reliable do client
/// Enviado no canal 0, message_id = 2
#[derive(Debug, Clone)]
pub struct MatrixAck {
    /// Proximo numero de sequencia esperado pelo servidor
    pub next_seq_num: u16,
    /// Numero de sequencia sendo confirmado
    pub ack_for_num: u16,
}

impl MatrixAck {
    /// Serializa para bytes (sem o message_id, que e adicionado pelo caller)
    pub fn serialize(&self) -> Vec<u8> {
        let mut buf = BytesMut::with_capacity(4);
        buf.put_u16(self.next_seq_num);
        buf.put_u16(self.ack_for_num);
        buf.to_vec()
    }

    /// Parseia a partir de bytes (sem o message_id)
    pub fn parse(data: &[u8]) -> Option<Self> {
        if data.len() < 4 {
            return None;
        }
        let mut buf = &data[..];
        Some(Self {
            next_seq_num: buf.get_u16(),
            ack_for_num: buf.get_u16(),
        })
    }
}

/// GSSAck - servidor confirma recebimento de pacote GSS reliable
/// Enviado no canal 0, message_id = 3
#[derive(Debug, Clone)]
pub struct GssAck {
    /// Proximo numero de sequencia esperado
    pub next_seq_num: u16,
    /// Numero de sequencia sendo confirmado
    pub ack_for_num: u16,
}

impl GssAck {
    pub fn serialize(&self) -> Vec<u8> {
        let mut buf = BytesMut::with_capacity(4);
        buf.put_u16(self.next_seq_num);
        buf.put_u16(self.ack_for_num);
        buf.to_vec()
    }

    pub fn parse(data: &[u8]) -> Option<Self> {
        if data.len() < 4 {
            return None;
        }
        let mut buf = &data[..];
        Some(Self {
            next_seq_num: buf.get_u16(),
            ack_for_num: buf.get_u16(),
        })
    }
}

/// TimeSyncRequest - client pede sincronizacao de tempo
/// Canal 0, message_id = 4
#[derive(Debug, Clone)]
pub struct TimeSyncRequest {
    /// Timestamp do client em microsegundos
    pub client_time: u64,
}

impl TimeSyncRequest {
    pub fn parse(data: &[u8]) -> Option<Self> {
        if data.len() < 8 {
            return None;
        }
        let mut buf = &data[..];
        // Dados de mensagem usam Little Endian (confirmado AeroMessages)
        Some(Self {
            client_time: buf.get_u64_le(),
        })
    }
}

/// TimeSyncResponse - servidor responde com tempo do servidor + echo do client
/// Canal 0, message_id = 5 (CTRL_TIME_SYNC_RESPONSE)
/// Formato confirmado por AeroMessages: ClientTime primeiro, ServerTime segundo
#[derive(Debug, Clone)]
pub struct TimeSyncResponse {
    /// Echo do client_time recebido (vem PRIMEIRO no wire)
    pub client_time: u64,
    /// Tempo do servidor em microsegundos (UNIX epoch)
    pub server_time: u64,
}

impl TimeSyncResponse {
    pub fn serialize(&self) -> Vec<u8> {
        let mut buf = BytesMut::with_capacity(16);
        // Dados de mensagem usam Little Endian (confirmado AeroMessages)
        buf.put_u64_le(self.client_time); // ClientTime primeiro
        buf.put_u64_le(self.server_time); // ServerTime segundo
        buf.to_vec()
    }
}

// ==================== Mensagens Reliable (Canal 1) ====================

/// Login message - primeiro pacote reliable enviado pelo client apos handshake
/// Formato confirmado AeroMessages/PIN (dados em Little Endian):
///   byte CharacterIsDev
///   uint32 ClientVersion
///   AeroString Unk2 (ushort LE len + UTF8)
///   uint64 CharacterGuid
///   ... campos adicionais (ignorados para MVP)
#[derive(Debug, Clone)]
pub struct LoginMessage {
    /// Se o personagem esta em modo dev
    pub character_is_dev: u8,
    /// Versao do client
    pub client_version: u32,
    /// String desconhecida 2
    pub unk2: String,
    /// GUID do personagem
    pub character_guid: u64,
    /// Bytes crus restantes (para debug/analise futura)
    pub raw_remaining: Vec<u8>,
}

impl LoginMessage {
    /// Helper para ler AeroString (ushort LE length prefix + UTF8 bytes)
    fn read_aero_string(buf: &mut &[u8]) -> Option<String> {
        if buf.len() < 2 {
            return None;
        }
        let len = buf.get_u16_le() as usize;
        if buf.len() < len {
            return None;
        }
        let s = String::from_utf8_lossy(&buf[..len]).to_string();
        buf.advance(len);
        Some(s)
    }

    /// Parseia mensagem Login do payload (apos seq + msg_id)
    /// Dados de mensagem usam Little Endian (confirmado AeroMessages)
    pub fn parse(data: &[u8]) -> Option<Self> {
        if data.len() < 5 {
            tracing::warn!(
                "LoginMessage muito curta: {} bytes (minimo 5)",
                data.len()
            );
            return None;
        }

        let mut buf = &data[..];
        let character_is_dev = buf.get_u8();
        let client_version = buf.get_u32_le();

        // AeroString Unk2
        let unk2 = Self::read_aero_string(&mut buf).unwrap_or_default();

        // uint64 CharacterGuid
        let character_guid = if buf.len() >= 8 {
            buf.get_u64_le()
        } else {
            tracing::warn!(
                "LoginMessage: sem bytes suficientes para CharacterGuid (restam {})",
                buf.len()
            );
            0
        };

        let raw_remaining = buf.to_vec();

        Some(Self {
            character_is_dev,
            client_version,
            unk2,
            character_guid,
            raw_remaining,
        })
    }
}

/// WelcomeToTheMatrix - primeira resposta do servidor apos receber Login
/// Enviada no canal 1 (reliable) com numero de sequencia
/// Formato confirmado AeroMessages: PlayerID + dois AeroArray(ushort) vazios
#[derive(Debug, Clone)]
pub struct WelcomeToTheMatrix {
    /// Entity ID do jogador no mundo (= character_guid)
    pub player_id: u64,
    /// Array 1 (desconhecido, enviar vazio para MVP)
    pub unk1: Vec<u8>,
    /// Array 2 (desconhecido, enviar vazio para MVP)
    pub unk2: Vec<u8>,
}

impl WelcomeToTheMatrix {
    pub fn serialize(&self) -> Vec<u8> {
        // uint64 PlayerID + ushort len1 + data1 + ushort len2 + data2
        let mut buf = BytesMut::with_capacity(12 + self.unk1.len() + self.unk2.len());
        buf.put_u64_le(self.player_id); // LE para dados de mensagem
        // AeroArray com length prefix ushort (Little Endian para dados de mensagem)
        buf.put_u16_le(self.unk1.len() as u16);
        if !self.unk1.is_empty() {
            buf.put_slice(&self.unk1);
        }
        buf.put_u16_le(self.unk2.len() as u16);
        if !self.unk2.is_empty() {
            buf.put_slice(&self.unk2);
        }
        buf.to_vec()
    }
}

/// EnterZone - servidor informa o client para entrar numa zona
/// Enviada no canal 1 (reliable) apos WelcomeToTheMatrix
/// Formato confirmado AeroMessages/PIN - struct complexo com sub-structs
#[derive(Debug, Clone)]
pub struct EnterZone {
    /// ID da instancia (uint64)
    pub instance_id: u64,
    /// ID da zona (uint32)
    pub zone_id: u32,
    /// Timestamp da zona (int64, microsegundos UNIX epoch)
    pub zone_timestamp: i64,
    /// Flags da zona: ZonePreview=1, DataPreview=2, AssetPreview=4
    pub zone_flags: u8,
    /// Dono da zona (AeroString: ushort len + UTF8)
    pub zone_owner: String,
    /// Streaming protocol (ushort)
    pub streaming_protocol: u16,
    /// SVN revision (uint32)
    pub svn_revision: u32,
    /// Hotfix level (byte)
    pub hotfix_level: u8,
    /// Match ID (uint64)
    pub match_id: u64,
    /// Unk2 (sbyte)
    pub unk2: i8,
    /// Simulation seed ms (uint32)
    pub simulation_seed_ms: u32,
    /// Nome da zona (AeroString)
    pub zone_name: String,
    /// Se tem DevZoneInfo (byte)
    pub have_dev_zone_info: bool,
    // ZoneTimeSyncData
    /// Fiction date time offset micros (int64)
    pub fiction_datetime_offset_micros: i64,
    /// Day length factor (float, default 12.0)
    pub day_length_factor: f32,
    /// Day phase offset (float)
    pub day_phase_offset: f32,
    // GameClockInfoData
    /// MicroUnix 1 (int64)
    pub game_clock_micro_1: i64,
    /// MicroUnix 2 (int64)
    pub game_clock_micro_2: i64,
    /// Timescale (float)
    pub game_clock_timescale: f32,
    /// GameClock unk1 (uint32)
    pub game_clock_unk1: u32,
    /// GameClock unk2 (uint32)
    pub game_clock_unk2: u32,
    /// Paused (byte)
    pub game_clock_paused: bool,
    /// Spectator mode flag (sbyte)
    pub spectator_mode_flag: i8,
}

impl EnterZone {
    /// Cria um EnterZone com valores padrao para MVP
    pub fn new_default(instance_id: u64, zone_id: u32, zone_name: &str) -> Self {
        let now_micros = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_micros() as i64;

        Self {
            instance_id,
            zone_id,
            zone_timestamp: now_micros,
            zone_flags: 0,
            zone_owner: String::new(),
            streaming_protocol: 0x4C5F, // mesmo do KISS
            svn_revision: 1962,
            hotfix_level: 0,
            match_id: 0,
            unk2: 0,
            simulation_seed_ms: 0,
            zone_name: zone_name.to_string(),
            have_dev_zone_info: false,
            fiction_datetime_offset_micros: 0,
            day_length_factor: 12.0,
            day_phase_offset: 0.0,
            game_clock_micro_1: now_micros,
            game_clock_micro_2: now_micros,
            game_clock_timescale: 1.0,
            game_clock_unk1: 0,
            game_clock_unk2: 0,
            game_clock_paused: false,
            spectator_mode_flag: 0,
        }
    }

    /// Serializa helper para AeroString (ushort LE length prefix + UTF8 bytes)
    fn write_aero_string(buf: &mut BytesMut, s: &str) {
        let bytes = s.as_bytes();
        buf.put_u16_le(bytes.len() as u16);
        buf.put_slice(bytes);
    }

    pub fn serialize(&self) -> Vec<u8> {
        let mut buf = BytesMut::with_capacity(256);

        // Campos principais (dados de mensagem usam Little Endian)
        buf.put_u64_le(self.instance_id);
        buf.put_u32_le(self.zone_id);
        buf.put_i64_le(self.zone_timestamp);
        buf.put_u8(self.zone_flags);
        Self::write_aero_string(&mut buf, &self.zone_owner);
        buf.put_u16_le(self.streaming_protocol);
        buf.put_u32_le(self.svn_revision);
        buf.put_u8(self.hotfix_level);
        buf.put_u64_le(self.match_id);
        buf.put_i8(self.unk2);
        buf.put_u32_le(self.simulation_seed_ms);
        Self::write_aero_string(&mut buf, &self.zone_name);

        // DevZoneInfo flag (0 = sem dev zone info)
        buf.put_u8(if self.have_dev_zone_info { 1 } else { 0 });

        // ZoneTimeSyncData (sempre presente)
        buf.put_i64_le(self.fiction_datetime_offset_micros);
        buf.put_f32_le(self.day_length_factor);
        buf.put_f32_le(self.day_phase_offset);

        // GameClockInfoData (sempre presente)
        buf.put_i64_le(self.game_clock_micro_1);
        buf.put_i64_le(self.game_clock_micro_2);
        buf.put_f32_le(self.game_clock_timescale);
        buf.put_u32_le(self.game_clock_unk1);
        buf.put_u32_le(self.game_clock_unk2);
        buf.put_u8(if self.game_clock_paused { 1 } else { 0 });

        // SpectatorModeFlag
        buf.put_i8(self.spectator_mode_flag);

        buf.to_vec()
    }
}

// ==================== Utilidades ====================

/// Cria o payload de um pacote de controle (canal 0) com message_id + dados
pub fn build_control_payload(message_id: u8, data: &[u8]) -> Vec<u8> {
    let mut buf = BytesMut::with_capacity(1 + data.len());
    buf.put_u8(message_id);
    buf.put_slice(data);
    buf.to_vec()
}

/// Cria o payload de um pacote reliable (canal 1) com seq + message_id + dados
pub fn build_reliable_payload(seq_num: u16, message_id: u8, data: &[u8]) -> Vec<u8> {
    let mut buf = BytesMut::with_capacity(3 + data.len());
    buf.put_u16(seq_num);
    buf.put_u8(message_id);
    buf.put_slice(data);
    buf.to_vec()
}

/// Extrai numero de sequencia e payload de um pacote reliable (canal 1)
/// Retorna (seq_num, message_id, message_data)
pub fn parse_reliable_payload(data: &[u8]) -> Option<(u16, u8, &[u8])> {
    if data.len() < 3 {
        tracing::warn!(
            "Payload reliable muito curto: {} bytes (minimo 3)",
            data.len()
        );
        return None;
    }
    let mut buf = &data[..];
    let seq_num = buf.get_u16();
    let message_id = buf.get_u8();
    let remaining = &data[3..];
    Some((seq_num, message_id, remaining))
}

/// Extrai message_id e dados de um pacote de controle (canal 0)
/// Retorna (message_id, message_data)
pub fn parse_control_payload(data: &[u8]) -> Option<(u8, &[u8])> {
    if data.is_empty() {
        tracing::warn!("Payload de controle vazio");
        return None;
    }
    let message_id = data[0];
    let remaining = &data[1..];
    Some((message_id, remaining))
}

// ==================== Testes ====================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_matrix_ack_roundtrip() {
        let ack = MatrixAck {
            next_seq_num: 5,
            ack_for_num: 4,
        };
        let bytes = ack.serialize();
        assert_eq!(bytes.len(), 4);

        let parsed = MatrixAck::parse(&bytes).unwrap();
        assert_eq!(parsed.next_seq_num, 5);
        assert_eq!(parsed.ack_for_num, 4);
    }

    #[test]
    fn test_gss_ack_roundtrip() {
        let ack = GssAck {
            next_seq_num: 10,
            ack_for_num: 9,
        };
        let bytes = ack.serialize();
        let parsed = GssAck::parse(&bytes).unwrap();
        assert_eq!(parsed.next_seq_num, 10);
        assert_eq!(parsed.ack_for_num, 9);
    }

    #[test]
    fn test_time_sync_response_serialize() {
        let resp = TimeSyncResponse {
            client_time: 987654321,
            server_time: 123456789,
        };
        let bytes = resp.serialize();
        assert_eq!(bytes.len(), 16);

        let mut buf = &bytes[..];
        // Little Endian, ClientTime primeiro (confirmado AeroMessages)
        assert_eq!(buf.get_u64_le(), 987654321);
        assert_eq!(buf.get_u64_le(), 123456789);
    }

    #[test]
    fn test_time_sync_request_parse() {
        let mut data = BytesMut::with_capacity(8);
        data.put_u64_le(555555555); // Little Endian
        let req = TimeSyncRequest::parse(&data).unwrap();
        assert_eq!(req.client_time, 555555555);
    }

    #[test]
    fn test_welcome_serialize() {
        let msg = WelcomeToTheMatrix {
            player_id: 0xDEADBEEF_12345678,
            unk1: vec![],
            unk2: vec![],
        };
        let bytes = msg.serialize();
        // uint64 PlayerID (8) + ushort 0 (2) + ushort 0 (2) = 12 bytes
        assert_eq!(bytes.len(), 12);

        let mut buf = &bytes[..];
        assert_eq!(buf.get_u64_le(), 0xDEADBEEF_12345678); // PlayerID (LE para dados de msg)
        assert_eq!(buf.get_u16_le(), 0); // unk1 length = 0
        assert_eq!(buf.get_u16_le(), 0); // unk2 length = 0
    }

    #[test]
    fn test_enter_zone_serialize() {
        let msg = EnterZone::new_default(0x100000001, 1, "TestZone");
        let bytes = msg.serialize();
        // Deve ter pelo menos os campos obrigatorios
        assert!(bytes.len() > 50, "EnterZone deve ter >50 bytes, tem {}", bytes.len());

        // Verificar primeiros campos (Little Endian para dados de mensagem)
        let mut buf = &bytes[..];
        let instance_id = buf.get_u64_le();
        assert_eq!(instance_id, 0x100000001);
        let zone_id = buf.get_u32_le();
        assert_eq!(zone_id, 1);
    }

    #[test]
    fn test_build_control_payload() {
        let data = vec![0x01, 0x02];
        let payload = build_control_payload(CTRL_MATRIX_ACK, &data);
        assert_eq!(payload, vec![CTRL_MATRIX_ACK, 0x01, 0x02]);
    }

    #[test]
    fn test_build_reliable_payload() {
        let data = vec![0xAA, 0xBB];
        let payload = build_reliable_payload(7, MSG_LOGIN, &data);
        // [uint16 seq=7] [uint8 msg_id=1] [0xAA] [0xBB]
        assert_eq!(payload, vec![0x00, 0x07, MSG_LOGIN, 0xAA, 0xBB]);
    }

    #[test]
    fn test_parse_reliable_payload() {
        let data = vec![0x00, 0x03, 0x01, 0xDE, 0xAD];
        let (seq, msg_id, remaining) = parse_reliable_payload(&data).unwrap();
        assert_eq!(seq, 3);
        assert_eq!(msg_id, 1);
        assert_eq!(remaining, &[0xDE, 0xAD]);
    }

    #[test]
    fn test_parse_control_payload() {
        let data = vec![0x04, 0x00, 0x00, 0x00, 0x01];
        let (msg_id, remaining) = parse_control_payload(&data).unwrap();
        assert_eq!(msg_id, 4);
        assert_eq!(remaining, &[0x00, 0x00, 0x00, 0x01]);
    }

    #[test]
    fn test_login_message_parse_minimal() {
        // Formato: [u8 isDev] [u32 LE version] [u16 LE str_len] [str bytes] [u64 LE guid]
        let mut data = BytesMut::with_capacity(32);
        data.put_u8(0); // isDev
        data.put_u32_le(1962); // version (Little Endian)
        data.put_u16_le(0); // AeroString unk2 vazia (len=0)
        data.put_u64_le(0xABCD_EF01_2345_6789); // guid (Little Endian)

        let msg = LoginMessage::parse(&data).unwrap();
        assert_eq!(msg.character_is_dev, 0);
        assert_eq!(msg.client_version, 1962);
        assert_eq!(msg.unk2, "");
        assert_eq!(msg.character_guid, 0xABCD_EF01_2345_6789);
    }

    #[test]
    fn test_parse_reliable_too_short() {
        assert!(parse_reliable_payload(&[0x00]).is_none());
        assert!(parse_reliable_payload(&[0x00, 0x01]).is_none());
    }

    #[test]
    fn test_parse_control_empty() {
        assert!(parse_control_payload(&[]).is_none());
    }
}

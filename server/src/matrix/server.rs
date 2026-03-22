// Matrix UDP Game Server - loop principal de rede
//
// Escuta pacotes UDP, gerencia o handshake POKE/HEHE/KISS/HUGG
// e processa pacotes de dados pos-handshake.
//
// O client conecta ao Matrix via System.StartInstanceConnection(matrix_url)
// onde matrix_url aponta para este servidor (ex: "127.0.0.1:25000")

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Instant;
use tokio::net::UdpSocket;

use super::gss;
use super::messages::{
    self, MatrixAck, TimeSyncRequest, TimeSyncResponse,
    WelcomeToTheMatrix, EnterZone, LoginMessage, MatrixStatus,
    CTRL_MATRIX_ACK, CTRL_GSS_ACK, CTRL_TIME_SYNC_REQUEST, CTRL_TIME_SYNC_RESPONSE,
    MSG_LOGIN, MSG_WELCOME_TO_THE_MATRIX, MSG_ENTER_ZONE, MSG_MATRIX_STATUS,
    MSG_ENTER_ZONE_ACK, MSG_CLIENT_STATUS, MSG_CLIENT_PREFERENCES,
    MSG_SUPER_PING, MSG_KEYFRAME_REQUEST,
};
use super::packet::{
    self, ClientPacket, ServerPacket, PROTOCOL_VERSION,
};
use super::session::{HandshakeState, SessionManager};

/// Tamanho maximo de um pacote UDP que vamos receber
const MAX_PACKET_SIZE: usize = 2048;

/// Porta padrao do game server (informada no HUGG)
/// No Firefall original, handshake e game data eram portas separadas (25000/25001).
/// No emulador, usamos a MESMA porta para tudo. O HUGG informa a porta publica
/// (MATRIX_PUBLIC_PORT via playit.gg tunnel) para que o client envie dados de volta.
const DEFAULT_GAME_PORT: u16 = 25000;

/// Intervalo de limpeza de sessoes expiradas (em segundos)
const CLEANUP_INTERVAL_SECS: u64 = 30;

/// Estado compartilhado do Matrix server
pub struct MatrixServer {
    /// Socket UDP principal
    socket: Arc<UdpSocket>,
    /// Gerenciador de sessoes
    sessions: SessionManager,
    /// Porta do game server (informada ao client no HUGG)
    game_port: u16,
    /// Timestamp de inicio do servidor (para time sync)
    start_time: Instant,
}

impl MatrixServer {
    /// Cria e inicia o Matrix server na porta especificada
    /// game_port: porta que o HUGG informa ao client para enviar dados de jogo
    ///   (pode ser diferente da porta de bind se usando tunnel/NAT)
    pub async fn bind(port: u16, game_port: u16) -> anyhow::Result<Self> {
        let bind_addr = format!("0.0.0.0:{}", port);
        let socket = UdpSocket::bind(&bind_addr).await?;
        tracing::info!(
            "Matrix UDP server escutando em {} (game port no HUGG: {})",
            bind_addr,
            game_port
        );

        Ok(Self {
            socket: Arc::new(socket),
            sessions: SessionManager::new(),
            game_port,
            start_time: Instant::now(),
        })
    }

    /// Loop principal: recebe e processa pacotes UDP
    pub async fn run(self) -> anyhow::Result<()> {
        let server = Arc::new(self);

        // Task de limpeza periodica de sessoes expiradas
        let cleanup_server = server.clone();
        tokio::spawn(async move {
            let mut interval =
                tokio::time::interval(std::time::Duration::from_secs(CLEANUP_INTERVAL_SECS));
            loop {
                interval.tick().await;
                let count = cleanup_server.sessions.cleanup_expired().await;
                if count > 0 {
                    tracing::debug!(
                        "Limpeza: {} sessoes expiradas, {} ativas",
                        count,
                        cleanup_server.sessions.session_count().await,
                    );
                }
            }
        });

        // Loop principal de recepcao de pacotes
        let mut buf = [0u8; MAX_PACKET_SIZE];
        loop {
            match server.socket.recv_from(&mut buf).await {
                Ok((len, addr)) => {
                    let data = &buf[..len];

                    // Log hex dump do pacote recebido
                    packet::log_received_hex(data, &addr);

                    // Processar pacote (nao bloqueia o loop principal)
                    let srv = server.clone();
                    let packet_data = data.to_vec();
                    tokio::spawn(async move {
                        if let Err(e) = srv.handle_packet(&packet_data, addr).await {
                            tracing::error!(
                                "Erro ao processar pacote de {}: {:?}",
                                addr,
                                e
                            );
                        }
                    });
                }
                Err(e) => {
                    // Erros de I/O no socket UDP normalmente nao sao fatais
                    tracing::error!("Erro ao receber pacote UDP: {:?}", e);
                }
            }
        }
    }

    /// Processa um pacote recebido de um client
    async fn handle_packet(
        &self,
        data: &[u8],
        addr: SocketAddr,
    ) -> anyhow::Result<()> {
        let client_packet = packet::parse_client_packet(data);

        match client_packet {
            ClientPacket::Poke { protocol_version } => {
                self.handle_poke(addr, protocol_version).await?;
            }
            ClientPacket::Kiss {
                socket_id,
                protocol_version,
                streaming_protocol,
            } => {
                self.handle_kiss(addr, socket_id, protocol_version, streaming_protocol)
                    .await?;
            }
            ClientPacket::Abort { socket_id } => {
                self.handle_abort(socket_id).await;
            }
            ClientPacket::Data {
                socket_id,
                channel,
                resend_count,
                is_split,
                payload,
            } => {
                self.handle_data(addr, socket_id, channel, resend_count, is_split, &payload)
                    .await?;
            }
            ClientPacket::Unknown { raw } => {
                tracing::warn!(
                    "Pacote desconhecido de {} ({} bytes), ignorando",
                    addr,
                    raw.len()
                );
            }
        }

        Ok(())
    }

    /// Trata POKE: client solicita conexao
    /// Responde com HEHE atribuindo um socket_id
    async fn handle_poke(
        &self,
        addr: SocketAddr,
        protocol_version: u32,
    ) -> anyhow::Result<()> {
        // Verificar versao do protocolo
        if protocol_version != PROTOCOL_VERSION {
            tracing::warn!(
                "POKE de {} com versao de protocolo inesperada: 0x{:08X} (esperado: 0x{:08X})",
                addr,
                protocol_version,
                PROTOCOL_VERSION
            );
            // Continuar mesmo assim - pode ser uma versao diferente do client
        }

        // Verificar se ja existe sessao para este endereco
        if let Some(existing) = self.sessions.get_session_by_addr(&addr).await {
            tracing::warn!(
                "POKE duplicado de {} (ja tem sessao 0x{:08X}), removendo sessao antiga",
                addr,
                existing.socket_id
            );
            self.sessions.remove_session(existing.socket_id).await;
        }

        // Criar nova sessao
        let socket_id = self.sessions.create_session(addr).await;

        // Enviar HEHE com o socket_id atribuido
        let response = ServerPacket::Hehe {
            assigned_socket_id: socket_id,
        };
        self.send_packet(&response, addr).await?;

        Ok(())
    }

    /// Trata KISS: client confirma protocolo
    /// Responde com HUGG para completar o handshake
    async fn handle_kiss(
        &self,
        addr: SocketAddr,
        socket_id: u32,
        protocol_version: u16,
        streaming_protocol: u16,
    ) -> anyhow::Result<()> {
        // Verificar se existe sessao para este socket_id
        let session = match self.sessions.get_session(socket_id).await {
            Some(s) => s,
            None => {
                tracing::warn!(
                    "KISS de {} com socket_id desconhecido: 0x{:08X}",
                    addr,
                    socket_id
                );
                return Ok(());
            }
        };

        // Verificar estado da sessao
        if session.state != HandshakeState::WaitingKiss {
            tracing::warn!(
                "KISS de {} mas sessao 0x{:08X} esta em estado {:?} (esperado: WaitingKiss)",
                addr,
                socket_id,
                session.state
            );
            // Continuar mesmo assim - pode ser um retry do client
        }

        tracing::info!(
            "KISS aceito: socket_id=0x{:08X}, proto=0x{:04X}, streaming=0x{:04X}",
            socket_id,
            protocol_version,
            streaming_protocol
        );

        // Atualizar estado para Connected
        self.sessions
            .update_state(socket_id, HandshakeState::Connected)
            .await;

        // Enviar HUGG para completar o handshake
        // sequence_start=1 (confirmado pelo PIN project)
        let response = ServerPacket::Hugg {
            socket_id,
            sequence_start: 1,
            game_server_port: self.game_port,
        };
        self.send_packet(&response, addr).await?;

        tracing::info!(
            "Handshake completo para {} (socket_id=0x{:08X}). Aguardando mensagens de gameplay...",
            addr,
            socket_id
        );

        Ok(())
    }

    /// Trata ABRT: client quer desconectar
    async fn handle_abort(&self, socket_id: u32) {
        tracing::info!(
            "ABRT recebido para socket_id=0x{:08X}, removendo sessao",
            socket_id
        );
        self.sessions.remove_session(socket_id).await;
    }

    /// Trata pacote de dados pos-handshake
    async fn handle_data(
        &self,
        addr: SocketAddr,
        socket_id: u32,
        channel: u8,
        _resend_count: u8,
        _is_split: bool,
        payload: &[u8],
    ) -> anyhow::Result<()> {
        // Verificar se existe sessao
        let session = match self.sessions.get_session(socket_id).await {
            Some(s) => s,
            None => {
                tracing::warn!(
                    "Pacote de dados de {} com socket_id desconhecido: 0x{:08X}",
                    addr,
                    socket_id
                );
                return Ok(());
            }
        };

        if session.state != HandshakeState::Connected {
            tracing::warn!(
                "Pacote de dados de {} mas sessao 0x{:08X} nao completou handshake (estado: {:?})",
                addr,
                socket_id,
                session.state
            );
            return Ok(());
        }

        // Atualizar atividade da sessao
        self.sessions.touch_session(socket_id).await;

        // Log detalhado do pacote de dados
        tracing::info!(
            "Dados recebidos: socket=0x{:08X}, canal={}, payload={} bytes",
            socket_id,
            channel,
            payload.len()
        );

        // Log hex dump completo do payload para analise do protocolo
        if !payload.is_empty() {
            let hex: String = payload
                .iter()
                .take(128)
                .map(|b| format!("{:02X}", b))
                .collect::<Vec<_>>()
                .join(" ");
            let suffix = if payload.len() > 128 { "..." } else { "" };
            tracing::info!(
                "  canal {} hex [{}B]: {}{}",
                channel,
                payload.len(),
                hex,
                suffix
            );
        }

        // Canal 0: controle (ACK/NACK/time sync)
        // Canal 1: reliable, mensagens abstratas (login, etc)
        // Canal 2: reliable + GSS header
        // Canal 3: unreliable + GSS header
        match channel {
            0 => {
                self.handle_channel0_control(addr, socket_id, payload).await?;
            }
            1 => {
                self.handle_channel1_reliable(addr, socket_id, payload).await?;
            }
            2 => {
                self.handle_channel2_reliable_gss(addr, socket_id, payload).await?;
            }
            3 => {
                self.handle_channel3_unreliable_gss(addr, socket_id, payload).await?;
            }
            _ => {
                tracing::warn!("Canal invalido {} de {}", channel, addr);
            }
        }

        Ok(())
    }

    // ==================== Handlers de Canal ====================

    /// Canal 0: mensagens de controle (ACK, TimeSync, keepalive)
    /// Formato: [uint8 MessageId] [message_data...]
    async fn handle_channel0_control(
        &self,
        addr: SocketAddr,
        socket_id: u32,
        payload: &[u8],
    ) -> anyhow::Result<()> {
        let (msg_id, msg_data) = match messages::parse_control_payload(payload) {
            Some(parsed) => parsed,
            None => {
                tracing::warn!(
                    "Canal 0: payload de controle vazio de {} (socket 0x{:08X})",
                    addr,
                    socket_id
                );
                return Ok(());
            }
        };

        tracing::debug!(
            "Canal 0: msg_id={}, data_len={} de {} (socket 0x{:08X})",
            msg_id,
            msg_data.len(),
            addr,
            socket_id
        );

        match msg_id {
            CTRL_MATRIX_ACK => {
                // Client enviando ACK para nossos pacotes reliable (canal 1)
                // DIAGNOSTICO: se client ACK seq=1 mas nao seq=2, EnterZone falhou na deserializacao
                if let Some(ack) = MatrixAck::parse(msg_data) {
                    tracing::info!(
                        "MatrixAck recebido: next_seq={}, ack_for={} de socket 0x{:08X} (seq1=Welcome, seq2=MatrixStatus, seq3=EnterZone)",
                        ack.next_seq_num,
                        ack.ack_for_num,
                        socket_id
                    );
                    // TODO: marcar pacotes como confirmados, parar retransmissao
                } else {
                    tracing::warn!(
                        "MatrixAck malformado: {} bytes de socket 0x{:08X}",
                        msg_data.len(),
                        socket_id
                    );
                }
            }
            CTRL_GSS_ACK => {
                // Client enviando ACK para pacotes GSS reliable (canal 2)
                if let Some(ack) = messages::GssAck::parse(msg_data) {
                    tracing::debug!(
                        "GssAck recebido: next_seq={}, ack_for={} de socket 0x{:08X}",
                        ack.next_seq_num,
                        ack.ack_for_num,
                        socket_id
                    );
                    // TODO: marcar pacotes GSS como confirmados
                } else {
                    tracing::warn!(
                        "GssAck malformado: {} bytes de socket 0x{:08X}",
                        msg_data.len(),
                        socket_id
                    );
                }
            }
            CTRL_TIME_SYNC_REQUEST => {
                // Fase 3: TimeSyncRequest do client
                self.handle_time_sync(addr, socket_id, msg_data).await?;
            }
            _ => {
                tracing::warn!(
                    "Canal 0: msg_id desconhecido {} ({} bytes) de socket 0x{:08X}",
                    msg_id,
                    msg_data.len(),
                    socket_id
                );
            }
        }

        Ok(())
    }

    /// Canal 1: mensagens reliable com numero de sequencia
    /// Formato: [uint16 SeqNum] [uint8 MessageId] [message_data...]
    async fn handle_channel1_reliable(
        &self,
        addr: SocketAddr,
        socket_id: u32,
        payload: &[u8],
    ) -> anyhow::Result<()> {
        let (seq_num, msg_id, msg_data) = match messages::parse_reliable_payload(payload) {
            Some(parsed) => parsed,
            None => {
                tracing::warn!(
                    "Canal 1: payload reliable muito curto ({} bytes) de {} (socket 0x{:08X})",
                    payload.len(),
                    addr,
                    socket_id
                );
                return Ok(());
            }
        };

        tracing::info!(
            "Canal 1 reliable: seq={}, msg_id=0x{:02X}, data_len={} de socket 0x{:08X}",
            seq_num,
            msg_id,
            msg_data.len(),
            socket_id
        );

        // Fase 1: Atualizar recv_seq e enviar ACK
        self.sessions.update_recv_seq(socket_id, seq_num).await;
        self.send_matrix_ack(addr, socket_id, seq_num).await?;

        // Processar mensagem pelo tipo
        match msg_id {
            MSG_LOGIN => {
                // Fase 2: Login do client
                self.handle_login(addr, socket_id, msg_data).await?;
            }
            MSG_ENTER_ZONE_ACK => {
                self.handle_enter_zone_ack(addr, socket_id, msg_data).await?;
            }
            MSG_CLIENT_STATUS => {
                // Status periodico do client (pode ignorar para MVP)
                tracing::debug!(
                    "ClientStatus recebido de socket 0x{:08X} ({} bytes)",
                    socket_id,
                    msg_data.len()
                );
            }
            MSG_CLIENT_PREFERENCES => {
                tracing::debug!(
                    "ClientPreferences recebido de socket 0x{:08X} ({} bytes)",
                    socket_id,
                    msg_data.len()
                );
            }
            MSG_KEYFRAME_REQUEST => {
                tracing::info!(
                    "KeyframeRequest recebido de socket 0x{:08X} ({} bytes)",
                    socket_id,
                    msg_data.len()
                );
                // TODO: enviar keyframe com estado atual do mundo
            }
            MSG_SUPER_PING => {
                tracing::debug!(
                    "SuperPing recebido de socket 0x{:08X} ({} bytes)",
                    socket_id,
                    msg_data.len()
                );
                // TODO: responder com SuperPong (ID 59)
            }
            _ => {
                tracing::warn!(
                    "Canal 1: msg_id desconhecido {} (0x{:02X}) (seq={}, {} bytes) de socket 0x{:08X}",
                    msg_id,
                    msg_id,
                    seq_num,
                    msg_data.len(),
                    socket_id
                );
                // Log hex do payload completo para analise futura
                let hex: String = msg_data
                    .iter()
                    .take(64)
                    .map(|b| format!("{:02X}", b))
                    .collect::<Vec<_>>()
                    .join(" ");
                tracing::info!("  msg_data hex: {}", hex);
            }
        }

        Ok(())
    }

    /// Canal 2: reliable + GSS header
    /// Formato: [uint16 SeqNum] [GSS header + data]
    async fn handle_channel2_reliable_gss(
        &self,
        addr: SocketAddr,
        socket_id: u32,
        payload: &[u8],
    ) -> anyhow::Result<()> {
        // Extrair numero de sequencia
        if payload.len() < 2 {
            tracing::warn!(
                "Canal 2: payload muito curto ({} bytes) de socket 0x{:08X}",
                payload.len(),
                socket_id
            );
            return Ok(());
        }

        let seq_num = u16::from_be_bytes([payload[0], payload[1]]);
        let gss_data = &payload[2..];

        tracing::debug!(
            "Canal 2 (reliable+GSS): seq={}, gss_data={} bytes de socket 0x{:08X}",
            seq_num,
            gss_data.len(),
            socket_id
        );

        // Enviar ACK para o pacote reliable (via canal 0, GssAck)
        self.send_gss_ack(addr, socket_id, seq_num).await?;

        // TODO: parsear GSS header e processar entidades/gameplay
        if !gss_data.is_empty() {
            let hex: String = gss_data
                .iter()
                .take(64)
                .map(|b| format!("{:02X}", b))
                .collect::<Vec<_>>()
                .join(" ");
            tracing::debug!("  GSS hex: {}", hex);
        }

        Ok(())
    }

    /// Canal 3: unreliable + GSS header (sem sequencia)
    async fn handle_channel3_unreliable_gss(
        &self,
        addr: SocketAddr,
        socket_id: u32,
        payload: &[u8],
    ) -> anyhow::Result<()> {
        tracing::debug!(
            "Canal 3 (unreliable+GSS): {} bytes de {} (socket 0x{:08X})",
            payload.len(),
            addr,
            socket_id
        );

        // TODO: parsear GSS header e processar (posicao, rotacao, etc)
        if !payload.is_empty() {
            let hex: String = payload
                .iter()
                .take(64)
                .map(|b| format!("{:02X}", b))
                .collect::<Vec<_>>()
                .join(" ");
            tracing::debug!("  GSS unreliable hex: {}", hex);
        }

        Ok(())
    }

    // ==================== Fase 2: Login Flow ====================

    /// Processa mensagem de Login do client (canal 1, msg_id = MSG_LOGIN)
    async fn handle_login(
        &self,
        addr: SocketAddr,
        socket_id: u32,
        msg_data: &[u8],
    ) -> anyhow::Result<()> {
        tracing::info!(
            "=== LOGIN recebido de {} (socket 0x{:08X}), {} bytes ===",
            addr,
            socket_id,
            msg_data.len()
        );

        // Log hex completo do login para analise
        let hex: String = msg_data
            .iter()
            .map(|b| format!("{:02X}", b))
            .collect::<Vec<_>>()
            .join(" ");
        tracing::info!("Login payload hex completo [{}B]: {}", msg_data.len(), hex);

        // Parsear o que conseguirmos
        let login = match LoginMessage::parse(msg_data) {
            Some(l) => l,
            None => {
                tracing::error!(
                    "Falha ao parsear LoginMessage de socket 0x{:08X}",
                    socket_id
                );
                return Ok(());
            }
        };

        tracing::info!(
            "Login parseado: is_dev={}, client_version={}, unk2='{}', character_guid=0x{:016X}",
            login.character_is_dev,
            login.client_version,
            login.unk2,
            login.character_guid
        );

        // Definir character_guid na sessao (usar fallback se for 0)
        let char_guid = if login.character_guid != 0 {
            login.character_guid
        } else {
            0xFFFF_0000_0000_0001
        };
        self.sessions.set_login_data(socket_id, char_guid).await;

        // Converter char_guid para entity_id (byte 0 = 0x00, formato PIN)
        let entity_id = gss::entity_id_from_guid(char_guid);

        // === FASE 2: Enviar sequencia de mensagens reliable ===

        // 1. WelcomeToTheMatrix com entity_id (byte 0 = 0x00)
        self.send_welcome_to_matrix(addr, socket_id, entity_id).await?;

        // Delay para client processar seq=1 antes de receber seq=2
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        // 2. MatrixStatus - informa ao client que o servidor esta pronto
        self.send_matrix_status(addr, socket_id).await?;

        // Delay para client processar
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        // 3. EnterZone - instrui client a carregar zona 448 (New Eden)
        let zone_id = 448;
        self.send_enter_zone(addr, socket_id, zone_id).await?;

        // 4. Enviar keyframes apos delay para client processar EnterZone
        // O client NAO envia EnterZoneAck como prerequisito para keyframes.
        // PIN envia keyframes imediatamente apos EnterZone.
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;
        self.send_initial_keyframes(addr, socket_id).await?;

        Ok(())
    }

    /// Envia WelcomeToTheMatrix para o client (canal 1, reliable)
    async fn send_welcome_to_matrix(
        &self,
        addr: SocketAddr,
        socket_id: u32,
        player_id: u64,
    ) -> anyhow::Result<()> {
        let welcome = WelcomeToTheMatrix {
            player_id,
            unk1: vec![], // Array vazio para MVP
            unk2: vec![], // Array vazio para MVP
        };

        tracing::info!(
            "Enviando WelcomeToTheMatrix: player_id=0x{:016X} para socket 0x{:08X}",
            player_id,
            socket_id
        );

        let msg_data = welcome.serialize();
        self.send_reliable(addr, socket_id, MSG_WELCOME_TO_THE_MATRIX, &msg_data).await
    }

    /// Envia EnterZone para o client (canal 1, reliable)
    async fn send_enter_zone(
        &self,
        addr: SocketAddr,
        socket_id: u32,
        zone_id: u32,
    ) -> anyhow::Result<()> {
        // Gerar instance_id: zone_id nos bits altos, instance number nos bits baixos
        // Formato: [zone_id:32][instance_num:32]
        let instance_id = ((zone_id as u64) << 32) | 1u64;

        let enter_zone = EnterZone::new_default(instance_id, zone_id, "New Eden");

        tracing::info!(
            "Enviando EnterZone: zone_id={}, instance=0x{:016X}, zone_name='{}' para socket 0x{:08X}",
            zone_id,
            instance_id,
            enter_zone.zone_name,
            socket_id
        );

        // Atualizar zona na sessao
        self.sessions.set_zone(socket_id, zone_id).await;

        let msg_data = enter_zone.serialize();

        // Logging diagnostico: hex dump completo do EnterZone
        tracing::info!(
            "EnterZone serializado [{}B]: {}",
            msg_data.len(),
            msg_data.iter().map(|b| format!("{:02X}", b)).collect::<Vec<_>>().join(" ")
        );

        self.send_reliable(addr, socket_id, MSG_ENTER_ZONE, &msg_data).await
    }

    /// Envia MatrixStatus para o client (canal 1, reliable)
    async fn send_matrix_status(
        &self,
        addr: SocketAddr,
        socket_id: u32,
    ) -> anyhow::Result<()> {
        tracing::info!("Enviando MatrixStatus para socket 0x{:08X}", socket_id);
        let msg_data = MatrixStatus::serialize();
        self.send_reliable(addr, socket_id, MSG_MATRIX_STATUS, &msg_data).await
    }

    /// Envia keyframes iniciais do personagem (BaseController, ObserverView, MovementView)
    async fn send_initial_keyframes(
        &self,
        addr: SocketAddr,
        socket_id: u32,
    ) -> anyhow::Result<()> {
        let session = match self.sessions.get_session(socket_id).await {
            Some(s) => s,
            None => {
                tracing::error!("send_initial_keyframes: sessao 0x{:08X} nao encontrada", socket_id);
                return Ok(());
            }
        };

        let character_guid = session.character_guid;
        if character_guid == 0 {
            tracing::error!("send_initial_keyframes: character_guid=0 para socket 0x{:08X}", socket_id);
            return Ok(());
        }

        let entity_id = gss::entity_id_from_guid(character_guid);
        tracing::info!(
            "Enviando keyframes iniciais: character_guid=0x{:016X}, entity_id=0x{:016X}",
            character_guid, entity_id
        );

        let spawn_pos: [f32; 3] = [297.0, 326.0, 434.0];

        // 1. BaseController Keyframe (controller_id=2, msg_id=4)
        let base_data = gss::build_base_controller_keyframe(character_guid, spawn_pos);
        tracing::info!("Enviando BaseController Keyframe: {} bytes", base_data.len());
        self.send_reliable_gss(addr, socket_id, gss::CTRL_CHARACTER_BASE, entity_id, gss::GSS_CONTROLLER_KEYFRAME, &base_data).await?;

        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        // 2. ObserverView Keyframe (controller_id=8, msg_id=3)
        let observer_data = gss::build_observer_view_keyframe("Player", 1, 0);
        tracing::info!("Enviando ObserverView Keyframe: {} bytes", observer_data.len());
        self.send_reliable_gss(addr, socket_id, gss::CTRL_CHARACTER_OBSERVER_VIEW, entity_id, gss::GSS_VIEW_KEYFRAME, &observer_data).await?;

        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        // 3. MovementView Keyframe (controller_id=12, msg_id=3)
        let spawn_rot: [f32; 4] = [0.0, 0.0, 0.0, 1.0];
        let spawn_aim: [f32; 3] = [1.0, 0.0, 0.0];
        let movement_data = gss::build_movement_view_keyframe(spawn_pos, spawn_rot, spawn_aim, 0x0010);
        tracing::info!("Enviando MovementView Keyframe: {} bytes", movement_data.len());
        self.send_reliable_gss(addr, socket_id, gss::CTRL_CHARACTER_MOVEMENT_VIEW, entity_id, gss::GSS_VIEW_KEYFRAME, &movement_data).await?;

        tracing::info!("=== Keyframes iniciais enviados para socket 0x{:08X} ===", socket_id);
        Ok(())
    }

    // ==================== Fase 3: Time Sync ====================

    /// Processa TimeSyncRequest do client e responde com TimeSyncResponse
    async fn handle_time_sync(
        &self,
        addr: SocketAddr,
        socket_id: u32,
        msg_data: &[u8],
    ) -> anyhow::Result<()> {
        let request = match TimeSyncRequest::parse(msg_data) {
            Some(r) => r,
            None => {
                tracing::warn!(
                    "TimeSyncRequest malformado: {} bytes de socket 0x{:08X}",
                    msg_data.len(),
                    socket_id
                );
                return Ok(());
            }
        };

        // ServerTime em microsegundos UNIX epoch (confirmado AeroMessages)
        let server_time_us = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_micros() as u64;

        tracing::debug!(
            "TimeSync: client_time={}, server_time={} para socket 0x{:08X}",
            request.client_time,
            server_time_us,
            socket_id
        );

        let response = TimeSyncResponse {
            client_time: request.client_time, // Echo do client time (vem primeiro)
            server_time: server_time_us,       // Server time UNIX epoch micros
        };

        let resp_data = response.serialize();
        // TimeSyncResponse usa message ID 5 (nao 4 como o Request)
        self.send_control(addr, socket_id, CTRL_TIME_SYNC_RESPONSE, &resp_data).await
    }

    // ==================== Fase 4: ACKs ====================

    /// Envia MatrixAck (canal 0, msg_id 2) confirmando recebimento de pacote reliable
    async fn send_matrix_ack(
        &self,
        addr: SocketAddr,
        socket_id: u32,
        received_seq: u16,
    ) -> anyhow::Result<()> {
        let ack = MatrixAck {
            next_seq_num: received_seq.wrapping_add(1),
            ack_for_num: received_seq,
        };

        tracing::debug!(
            "Enviando MatrixAck: next_seq={}, ack_for={} para socket 0x{:08X}",
            ack.next_seq_num,
            ack.ack_for_num,
            socket_id
        );

        let ack_data = ack.serialize();
        self.send_control(addr, socket_id, CTRL_MATRIX_ACK, &ack_data).await
    }

    /// Envia GssAck (canal 0, msg_id 3) confirmando recebimento de pacote GSS reliable
    async fn send_gss_ack(
        &self,
        addr: SocketAddr,
        socket_id: u32,
        received_seq: u16,
    ) -> anyhow::Result<()> {
        let ack = messages::GssAck {
            next_seq_num: received_seq.wrapping_add(1),
            ack_for_num: received_seq,
        };

        tracing::debug!(
            "Enviando GssAck: next_seq={}, ack_for={} para socket 0x{:08X}",
            ack.next_seq_num,
            ack.ack_for_num,
            socket_id
        );

        let ack_data = ack.serialize();
        self.send_control(addr, socket_id, CTRL_GSS_ACK, &ack_data).await
    }

    // ==================== Fase 5: GSS Entity Spawn ====================

    /// Processa EnterZoneAck do client (msg_id=18)
    /// Se o client envia esta mensagem, a zona carregou com sucesso.
    /// Os keyframes ja foram enviados por send_initial_keyframes, entao
    /// aqui apenas logamos o sucesso.
    async fn handle_enter_zone_ack(
        &self,
        _addr: SocketAddr,
        socket_id: u32,
        msg_data: &[u8],
    ) -> anyhow::Result<()> {
        tracing::info!(
            "=== EnterZoneAck RECEBIDO de socket 0x{:08X} ({} bytes) - ZONA CARREGADA COM SUCESSO! ===",
            socket_id,
            msg_data.len()
        );

        Ok(())
    }

    /// Envia um pacote GSS reliable (canal 2) com numero de sequencia e GSS header
    /// Formato: [uint32 socket_id] [uint16 packet_header] [uint16 seq_num BE] [9B GSS header] [data LE]
    async fn send_reliable_gss(
        &self,
        addr: SocketAddr,
        socket_id: u32,
        controller_id: u8,
        entity_id: u64,
        message_id: u8,
        data: &[u8],
    ) -> anyhow::Result<()> {
        let seq = match self.sessions.next_gss_send_seq(socket_id).await {
            Some(s) => s,
            None => {
                tracing::error!(
                    "Sessao 0x{:08X} nao encontrada para envio GSS reliable",
                    socket_id
                );
                return Ok(());
            }
        };

        tracing::info!(
            "Enviando GSS reliable: seq={}, ctrl={}, entity=0x{:016X}, msg_id={}, data_len={} para socket 0x{:08X}",
            seq,
            controller_id,
            entity_id,
            message_id,
            data.len(),
            socket_id
        );

        // Construir payload completo do canal 2: [seq BE] [GSS header] [data]
        let payload = gss::build_gss_payload(seq, controller_id, entity_id, message_id, data);

        // Log hex do payload GSS para debug
        let hex: String = payload
            .iter()
            .take(128)
            .map(|b| format!("{:02X}", b))
            .collect::<Vec<_>>()
            .join(" ");
        let suffix = if payload.len() > 128 { "..." } else { "" };
        tracing::debug!("GSS payload [{}B]: {}{}", payload.len(), hex, suffix);

        // Enviar como pacote de dados no canal 2
        let packet = ServerPacket::Data {
            socket_id,
            channel: 2,
            payload,
        };
        self.send_packet(&packet, addr).await?;
        self.sessions.mark_sent(socket_id).await;

        Ok(())
    }

    // ==================== Envio de Pacotes ====================

    /// Envia um pacote de controle (canal 0) para o client
    /// Formato: [uint8 message_id] [data...]
    async fn send_control(
        &self,
        addr: SocketAddr,
        socket_id: u32,
        message_id: u8,
        data: &[u8],
    ) -> anyhow::Result<()> {
        let payload = messages::build_control_payload(message_id, data);
        let packet = ServerPacket::Data {
            socket_id,
            channel: 0,
            payload,
        };
        self.send_packet(&packet, addr).await?;
        self.sessions.mark_sent(socket_id).await;
        Ok(())
    }

    /// Envia um pacote reliable (canal 1) com numero de sequencia
    /// Formato: [uint16 seq_num] [uint8 message_id] [data...]
    async fn send_reliable(
        &self,
        addr: SocketAddr,
        socket_id: u32,
        message_id: u8,
        data: &[u8],
    ) -> anyhow::Result<()> {
        let seq = match self.sessions.next_send_seq(socket_id).await {
            Some(s) => s,
            None => {
                tracing::error!(
                    "Sessao 0x{:08X} nao encontrada para envio reliable",
                    socket_id
                );
                return Ok(());
            }
        };

        tracing::info!(
            "Enviando reliable: seq={}, msg_id=0x{:02X}, data_len={} para socket 0x{:08X}",
            seq,
            message_id,
            data.len(),
            socket_id
        );

        let payload = messages::build_reliable_payload(seq, message_id, data);
        let packet = ServerPacket::Data {
            socket_id,
            channel: 1,
            payload,
        };
        self.send_packet(&packet, addr).await?;
        self.sessions.mark_sent(socket_id).await;
        Ok(())
    }

    /// Envia um pacote serializado para um endereco
    async fn send_packet(
        &self,
        packet: &ServerPacket,
        addr: SocketAddr,
    ) -> anyhow::Result<()> {
        let data = packet::serialize_server_packet(packet);
        let sent = self.socket.send_to(&data, addr).await?;
        tracing::debug!(
            "Enviado {} bytes para {}",
            sent,
            addr
        );
        Ok(())
    }
}

/// Ponto de entrada: inicia o Matrix server como task async
/// port: porta local para bind UDP
/// game_port: porta informada no HUGG (pode ser diferente se usando tunnel)
pub async fn start(port: u16, game_port: u16) {
    tracing::info!("=== Iniciando Matrix UDP Game Server na porta {} (HUGG game_port={}) ===", port, game_port);

    match MatrixServer::bind(port, game_port).await {
        Ok(server) => {
            if let Err(e) = server.run().await {
                tracing::error!("Matrix server encerrado com erro: {:?}", e);
            }
        }
        Err(e) => {
            tracing::error!("Falha ao iniciar Matrix server na porta {}: {:?}", port, e);
        }
    }
}

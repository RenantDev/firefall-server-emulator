// Matrix UDP Game Server - loop principal de rede
//
// Escuta pacotes UDP, gerencia o handshake POKE/HEHE/KISS/HUGG
// e processa pacotes de dados pos-handshake.
//
// O client conecta ao Matrix via System.StartInstanceConnection(matrix_url)
// onde matrix_url aponta para este servidor (ex: "127.0.0.1:25000")

use std::net::SocketAddr;
use std::sync::Arc;
use tokio::net::UdpSocket;

use super::packet::{
    self, ClientPacket, ServerPacket, PROTOCOL_VERSION,
};
use super::session::{HandshakeState, SessionManager};

/// Tamanho maximo de um pacote UDP que vamos receber
const MAX_PACKET_SIZE: usize = 2048;

/// Porta padrao do game server (informada no HUGG)
const DEFAULT_GAME_PORT: u16 = 25001;

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
}

impl MatrixServer {
    /// Cria e inicia o Matrix server na porta especificada
    pub async fn bind(port: u16) -> anyhow::Result<Self> {
        let bind_addr = format!("0.0.0.0:{}", port);
        let socket = UdpSocket::bind(&bind_addr).await?;
        tracing::info!(
            "Matrix UDP server escutando em {} (game port: {})",
            bind_addr,
            DEFAULT_GAME_PORT
        );

        Ok(Self {
            socket: Arc::new(socket),
            sessions: SessionManager::new(),
            game_port: DEFAULT_GAME_PORT,
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
        let response = ServerPacket::Hugg {
            socket_id,
            sequence_start: 0,
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

        // Canal 0: controle (ACK/NACK/time sync)
        // Canal 1: reliable, mensagens abstratas (login, etc)
        // Canal 2: reliable + GSS header
        // Canal 3: unreliable + GSS header
        match channel {
            0 => {
                // Canal de controle - pode ser time sync, ACK, NACK
                tracing::debug!(
                    "Canal 0 (controle): {} bytes de {}",
                    payload.len(),
                    addr
                );
                // TODO: implementar respostas de controle (ACK, time sync)
            }
            1 => {
                // Canal reliable - mensagens como login
                tracing::info!(
                    "Canal 1 (reliable): {} bytes de {} - provavelmente mensagem de login",
                    payload.len(),
                    addr
                );
                // TODO: parsear mensagens reliable (login, etc)
            }
            2 => {
                // Canal reliable + GSS
                tracing::debug!(
                    "Canal 2 (reliable+GSS): {} bytes de {}",
                    payload.len(),
                    addr
                );
                // TODO: parsear GSS messages
            }
            3 => {
                // Canal unreliable + GSS
                tracing::debug!(
                    "Canal 3 (unreliable+GSS): {} bytes de {}",
                    payload.len(),
                    addr
                );
                // TODO: parsear GSS messages
            }
            _ => {
                tracing::warn!("Canal invalido {} de {}", channel, addr);
            }
        }

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
pub async fn start(port: u16) {
    tracing::info!("=== Iniciando Matrix UDP Game Server na porta {} ===", port);

    match MatrixServer::bind(port).await {
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

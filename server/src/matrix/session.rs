// Gerenciamento de sessoes de clientes conectados ao Matrix server
//
// Cada client que completa o handshake POKE->HEHE->KISS->HUGG recebe uma sessao.
// A sessao armazena o estado da conexao e dados do jogador.

use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::RwLock;

/// Estado do handshake da conexao
#[derive(Debug, Clone, PartialEq)]
pub enum HandshakeState {
    /// Aguardando POKE do client
    WaitingPoke,
    /// HEHE enviado, aguardando KISS
    WaitingKiss,
    /// HUGG enviado, handshake completo
    Connected,
    /// Conexao sendo encerrada
    Disconnecting,
}

/// Sessao de um client conectado
#[derive(Debug, Clone)]
pub struct ClientSession {
    /// ID unico do socket atribuido pelo servidor
    pub socket_id: u32,
    /// Endereco UDP do client
    pub addr: SocketAddr,
    /// Estado atual do handshake
    pub state: HandshakeState,
    /// Versao do protocolo informada pelo client
    pub protocol_version: Option<u32>,
    /// Timestamp da ultima atividade (para timeout)
    pub last_activity: Instant,
    /// Timestamp de criacao da sessao
    pub created_at: Instant,
    /// Contador de pacotes recebidos (para estatisticas)
    pub packets_received: u64,
    /// Contador de pacotes enviados
    pub packets_sent: u64,

    // === Campos de sequencia (Fase 1) ===

    /// Proximo numero de sequencia a enviar (reliable channel 1)
    pub send_seq: u16,
    /// Proximo numero de sequencia esperado do client
    pub recv_seq: u16,
    /// Proximo numero de sequencia a enviar no canal 2 (GSS reliable)
    pub gss_send_seq: u16,

    // === Campos de gameplay (Fase 2) ===

    /// GUID do personagem (definido apos Login)
    pub character_guid: u64,
    /// ID da zona atual (definido apos EnterZone)
    pub zone_id: u32,
    /// Se o login ja foi processado
    pub login_received: bool,
}

impl ClientSession {
    /// Cria uma nova sessao para um client que enviou POKE
    pub fn new(socket_id: u32, addr: SocketAddr) -> Self {
        let now = Instant::now();
        Self {
            socket_id,
            addr,
            state: HandshakeState::WaitingKiss, // HEHE ja foi enviado
            protocol_version: None,
            last_activity: now,
            created_at: now,
            packets_received: 1, // contando o POKE
            packets_sent: 1,     // contando o HEHE
            send_seq: 1, // Comeca em 1 (corresponde ao seq_start no HUGG)
            recv_seq: 0,
            gss_send_seq: 1, // GSS canal 2 tambem comeca em 1
            character_guid: 0,
            zone_id: 0,
            login_received: false,
        }
    }

    /// Atualiza o timestamp de ultima atividade
    pub fn touch(&mut self) {
        self.last_activity = Instant::now();
        self.packets_received += 1;
    }

    /// Incrementa contador de pacotes enviados
    pub fn mark_sent(&mut self) {
        self.packets_sent += 1;
    }

    /// Verifica se a sessao expirou (sem atividade por mais de timeout_secs)
    pub fn is_expired(&self, timeout_secs: u64) -> bool {
        self.last_activity.elapsed().as_secs() > timeout_secs
    }

    /// Tempo de vida da sessao em segundos
    pub fn age_secs(&self) -> u64 {
        self.created_at.elapsed().as_secs()
    }
}

/// Gerenciador de sessoes - thread-safe via RwLock
#[derive(Clone)]
pub struct SessionManager {
    /// Mapa de socket_id -> sessao
    sessions_by_id: Arc<RwLock<HashMap<u32, ClientSession>>>,
    /// Mapa de endereco -> socket_id (para lookup rapido por endereco)
    addr_to_id: Arc<RwLock<HashMap<SocketAddr, u32>>>,
    /// Proximo socket_id a ser atribuido
    next_socket_id: Arc<RwLock<u32>>,
    /// Timeout de sessao em segundos (default: 60s)
    pub session_timeout_secs: u64,
}

impl SessionManager {
    /// Cria um novo gerenciador de sessoes
    pub fn new() -> Self {
        Self {
            sessions_by_id: Arc::new(RwLock::new(HashMap::new())),
            addr_to_id: Arc::new(RwLock::new(HashMap::new())),
            // Comecar com socket_id = 1 (0 e reservado para handshake)
            next_socket_id: Arc::new(RwLock::new(1)),
            session_timeout_secs: 60,
        }
    }

    /// Cria uma nova sessao para um client e retorna o socket_id atribuido
    pub async fn create_session(&self, addr: SocketAddr) -> u32 {
        let mut next_id = self.next_socket_id.write().await;
        let socket_id = *next_id;
        *next_id = next_id.wrapping_add(1);
        if *next_id == 0 {
            *next_id = 1; // pular 0, que e reservado
        }
        drop(next_id);

        let session = ClientSession::new(socket_id, addr);

        tracing::info!(
            "Nova sessao criada: socket_id=0x{:08X}, addr={}",
            socket_id,
            addr
        );

        self.sessions_by_id
            .write()
            .await
            .insert(socket_id, session);
        self.addr_to_id.write().await.insert(addr, socket_id);

        socket_id
    }

    /// Busca sessao por socket_id
    pub async fn get_session(&self, socket_id: u32) -> Option<ClientSession> {
        self.sessions_by_id.read().await.get(&socket_id).cloned()
    }

    /// Busca sessao por endereco do client
    pub async fn get_session_by_addr(&self, addr: &SocketAddr) -> Option<ClientSession> {
        let id = self.addr_to_id.read().await.get(addr).copied();
        match id {
            Some(socket_id) => self.get_session(socket_id).await,
            None => None,
        }
    }

    /// Atualiza o estado do handshake de uma sessao
    pub async fn update_state(&self, socket_id: u32, new_state: HandshakeState) {
        if let Some(session) = self.sessions_by_id.write().await.get_mut(&socket_id) {
            tracing::debug!(
                "Sessao 0x{:08X}: {:?} -> {:?}",
                socket_id,
                session.state,
                new_state
            );
            session.state = new_state;
            session.touch();
        }
    }

    /// Registra atividade em uma sessao (atualiza timestamp e contadores)
    pub async fn touch_session(&self, socket_id: u32) {
        if let Some(session) = self.sessions_by_id.write().await.get_mut(&socket_id) {
            session.touch();
        }
    }

    /// Registra que um pacote foi enviado para a sessao
    pub async fn mark_sent(&self, socket_id: u32) {
        if let Some(session) = self.sessions_by_id.write().await.get_mut(&socket_id) {
            session.mark_sent();
        }
    }

    /// Incrementa e retorna o proximo send_seq para enviar pacote reliable
    pub async fn next_send_seq(&self, socket_id: u32) -> Option<u16> {
        if let Some(session) = self.sessions_by_id.write().await.get_mut(&socket_id) {
            let seq = session.send_seq;
            session.send_seq = session.send_seq.wrapping_add(1);
            Some(seq)
        } else {
            None
        }
    }

    /// Incrementa e retorna o proximo gss_send_seq para enviar pacote no canal 2
    pub async fn next_gss_send_seq(&self, socket_id: u32) -> Option<u16> {
        if let Some(session) = self.sessions_by_id.write().await.get_mut(&socket_id) {
            let seq = session.gss_send_seq;
            session.gss_send_seq = session.gss_send_seq.wrapping_add(1);
            Some(seq)
        } else {
            None
        }
    }

    /// Atualiza recv_seq apos receber pacote reliable do client
    pub async fn update_recv_seq(&self, socket_id: u32, received_seq: u16) {
        if let Some(session) = self.sessions_by_id.write().await.get_mut(&socket_id) {
            // Aceitar o proximo na sequencia, ou qualquer seq se for o primeiro
            session.recv_seq = received_seq.wrapping_add(1);
        }
    }

    /// Marca que o login foi recebido e define character_guid
    pub async fn set_login_data(&self, socket_id: u32, character_guid: u64) {
        if let Some(session) = self.sessions_by_id.write().await.get_mut(&socket_id) {
            session.character_guid = character_guid;
            session.login_received = true;
            tracing::info!(
                "Sessao 0x{:08X}: login recebido, character_guid=0x{:016X}",
                socket_id,
                character_guid
            );
        }
    }

    /// Define a zona atual do jogador
    pub async fn set_zone(&self, socket_id: u32, zone_id: u32) {
        if let Some(session) = self.sessions_by_id.write().await.get_mut(&socket_id) {
            session.zone_id = zone_id;
            tracing::info!(
                "Sessao 0x{:08X}: entrou na zona {}",
                socket_id,
                zone_id
            );
        }
    }

    /// Remove uma sessao
    pub async fn remove_session(&self, socket_id: u32) {
        let session = self.sessions_by_id.write().await.remove(&socket_id);
        if let Some(session) = session {
            self.addr_to_id.write().await.remove(&session.addr);
            tracing::info!(
                "Sessao removida: socket_id=0x{:08X}, addr={}, durou {}s, rx={} tx={}",
                socket_id,
                session.addr,
                session.age_secs(),
                session.packets_received,
                session.packets_sent,
            );
        }
    }

    /// Remove sessoes expiradas e retorna quantas foram removidas
    pub async fn cleanup_expired(&self) -> usize {
        let timeout = self.session_timeout_secs;
        let mut sessions = self.sessions_by_id.write().await;
        let mut addr_map = self.addr_to_id.write().await;

        let expired_ids: Vec<u32> = sessions
            .iter()
            .filter(|(_, s)| s.is_expired(timeout))
            .map(|(id, _)| *id)
            .collect();

        let count = expired_ids.len();

        for id in &expired_ids {
            if let Some(session) = sessions.remove(id) {
                addr_map.remove(&session.addr);
                tracing::info!(
                    "Sessao expirada removida: socket_id=0x{:08X}, addr={}, inativa por >{}s",
                    id,
                    session.addr,
                    timeout,
                );
            }
        }

        if count > 0 {
            tracing::info!("{} sessoes expiradas removidas", count);
        }

        count
    }

    /// Retorna o numero total de sessoes ativas
    pub async fn session_count(&self) -> usize {
        self.sessions_by_id.read().await.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_create_session() {
        let manager = SessionManager::new();
        let addr: SocketAddr = "127.0.0.1:12345".parse().unwrap();

        let id = manager.create_session(addr).await;
        assert_eq!(id, 1); // primeiro ID atribuido

        let session = manager.get_session(id).await.unwrap();
        assert_eq!(session.addr, addr);
        assert_eq!(session.state, HandshakeState::WaitingKiss);

        assert_eq!(manager.session_count().await, 1);
    }

    #[tokio::test]
    async fn test_lookup_by_addr() {
        let manager = SessionManager::new();
        let addr: SocketAddr = "127.0.0.1:12345".parse().unwrap();

        let id = manager.create_session(addr).await;
        let session = manager.get_session_by_addr(&addr).await.unwrap();
        assert_eq!(session.socket_id, id);
    }

    #[tokio::test]
    async fn test_remove_session() {
        let manager = SessionManager::new();
        let addr: SocketAddr = "127.0.0.1:12345".parse().unwrap();

        let id = manager.create_session(addr).await;
        assert_eq!(manager.session_count().await, 1);

        manager.remove_session(id).await;
        assert_eq!(manager.session_count().await, 0);
        assert!(manager.get_session(id).await.is_none());
        assert!(manager.get_session_by_addr(&addr).await.is_none());
    }

    #[tokio::test]
    async fn test_multiple_sessions() {
        let manager = SessionManager::new();
        let addr1: SocketAddr = "127.0.0.1:10001".parse().unwrap();
        let addr2: SocketAddr = "127.0.0.1:10002".parse().unwrap();

        let id1 = manager.create_session(addr1).await;
        let id2 = manager.create_session(addr2).await;

        assert_ne!(id1, id2);
        assert_eq!(manager.session_count().await, 2);
    }

    #[tokio::test]
    async fn test_update_state() {
        let manager = SessionManager::new();
        let addr: SocketAddr = "127.0.0.1:12345".parse().unwrap();

        let id = manager.create_session(addr).await;
        manager
            .update_state(id, HandshakeState::Connected)
            .await;

        let session = manager.get_session(id).await.unwrap();
        assert_eq!(session.state, HandshakeState::Connected);
    }
}

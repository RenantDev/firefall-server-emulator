// Matrix game server module - servidor UDP de gameplay em tempo real
//
// O Matrix server gerencia:
// - Conexoes de jogadores via UDP (System.StartInstanceConnection)
// - Handshake de 4 passos: POKE -> HEHE -> KISS -> HUGG
// - Estado do mundo, entidades, movimento
// - Combate, habilidades, chat
//
// Protocolo binario Big Endian baseado em RE do client Firefall build 1962.

pub mod gss;
pub mod messages;
pub mod packet;
pub mod server;
pub mod session;

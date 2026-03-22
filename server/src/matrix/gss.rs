// GSS (Game State Stream) - sistema de entidades e keyframes
//
// O GSS usa o canal 2 (reliable) para enviar estado de entidades ao client.
// Cada entidade tem um entity_id (u64) e multiplos controllers, cada um com
// seu proprio controller_id. No wire, o byte 0 do entity_id e substituido
// pelo controller_id.
//
// Formato do payload no canal 2:
//   [2B seq_num BE] [9B GSS header] [message data LE]
//
// GSS Header (9 bytes):
//   [1B controller_id]       - sobrescreve byte 0 do entity_id
//   [7B entity_id[1..7] LE]  - bytes 1-7 do entity_id como little-endian u64
//   [1B message_id]          - tipo de mensagem GSS

use bytes::{BufMut, BytesMut};

// ==================== GSS Message IDs ====================

/// Update parcial de estado de um controller
pub const GSS_UPDATE: u8 = 1;
/// Keyframe de view (ObserverView, MovementView, etc)
pub const GSS_VIEW_KEYFRAME: u8 = 3;
/// Keyframe de controller (BaseController, etc)
pub const GSS_CONTROLLER_KEYFRAME: u8 = 4;

// ==================== Controller IDs ====================
// Cada controller gerencia um aspecto diferente da entidade

/// BaseController - estado base do personagem (vida, escudo, estado, etc)
pub const CTRL_CHARACTER_BASE: u8 = 2;
/// ObserverView - informacoes visiveis a outros jogadores (nome, visuais)
pub const CTRL_CHARACTER_OBSERVER_VIEW: u8 = 8;
/// MovementView - posicao, rotacao, velocidade, estado de movimento
pub const CTRL_CHARACTER_MOVEMENT_VIEW: u8 = 12;

// ==================== GSS Header ====================

/// Constroi o header GSS de 9 bytes
/// O byte 0 do entity_id e SUBSTITUIDO pelo controller_id no wire
pub fn build_gss_header(controller_id: u8, entity_id: u64, message_id: u8) -> [u8; 9] {
    let mut header = [0u8; 9];
    let id_bytes = entity_id.to_le_bytes(); // 8 bytes LE
    header[0] = controller_id;              // sobrescreve byte 0
    header[1..8].copy_from_slice(&id_bytes[1..8]); // copia bytes 1-7
    header[8] = message_id;
    header
}

/// Constroi o payload completo do canal 2 (seq_num BE + GSS header + data)
pub fn build_gss_payload(
    seq_num: u16,
    controller_id: u8,
    entity_id: u64,
    message_id: u8,
    data: &[u8],
) -> Vec<u8> {
    let header = build_gss_header(controller_id, entity_id, message_id);
    let mut buf = BytesMut::with_capacity(2 + 9 + data.len());
    buf.put_u16(seq_num);       // seq_num Big Endian
    buf.put_slice(&header);     // 9 bytes GSS header
    buf.put_slice(data);        // message data (LE)
    buf.to_vec()
}

// ==================== Keyframe Builders ====================

/// Constroi o entity_id a partir do character_guid
/// O byte 0 e mascarado pois sera substituido pelo controller_id no wire
pub fn entity_id_from_guid(character_guid: u64) -> u64 {
    character_guid & 0xFFFFFFFF_FFFFFF00
}

/// Constroi um BaseController Keyframe minimo (controller_id=2, msg_id=4)
///
/// Este e o keyframe mais complexo. Para MVP, enviamos dados minimos
/// para que o client nao crashe. O formato baseia-se no AeroMessages.
///
/// Estrutura minima:
/// - 8B player_guid LE (prefixo de controller keyframes)
/// - Campos com nullable bitfields indicando quais campos estao presentes
///
/// Para MVP ultra-minimo, tentamos enviar apenas o player_guid + bitfields zerados
/// indicando "nenhum campo presente". Se o client crashar, iteramos.
pub fn build_base_controller_keyframe(
    character_guid: u64,
    position: [f32; 3],
) -> Vec<u8> {
    let mut buf = BytesMut::with_capacity(256);

    // Prefixo: player_guid (8 bytes LE) - identifica o dono do controller
    buf.put_u64_le(character_guid);

    // === Nullable bitfields ===
    // O BaseController tem muitos campos opcionais controlados por bitfields.
    // Cada grupo de ate 8 campos nullable tem 1 byte de bitfield.
    // Bit = 1 significa "campo presente", bit = 0 significa "ausente/default".
    //
    // Para MVP, tentamos enviar os campos MINIMOS necessarios.
    // Baseado na analise do PIN project e AeroMessages:
    //
    // Bitfield 1 (campos 0-7):
    //   bit 0: CharacterState (NECESSARIO para nao crashar)
    //   bit 1: HostilityInfo
    //   bit 2: PersonalFactionStance
    //   bit 3: CurrentHealth
    //   bit 4: MaxHealth
    //   bit 5: CurrentShields
    //   bit 6: MaxShields
    //   bit 7: HealthRegenDelay (ou EmoteID em versoes diferentes)
    //
    // Bitfield 2 (campos 8-15):
    //   bit 0: SpawnTime / GibVisualsId
    //   bit 1: SpawnPose
    //   ... etc
    //
    // Tentativa: enviar CharacterState + CurrentHealth + MaxHealth + CurrentShields + MaxShields + SpawnPose

    // Bitfield 1: bits 0,3,4,5,6 = CharacterState + Health/Shield values
    buf.put_u8(0b0111_1001); // bits 0,3,4,5,6

    // Bitfield 2: bit 1 = SpawnPose
    buf.put_u8(0b0000_0010);

    // Bitfield 3: nenhum campo extra
    buf.put_u8(0b0000_0000);

    // === Campo 0: CharacterState ===
    // CharacterState e um enum: 0=None, 2=Spawning, 4=PreSpawn, 6=Living, 8=Dead, etc
    // Precisamos: state byte + time u32
    buf.put_u8(6); // CharacterState = Living
    buf.put_u32_le(0); // time = 0 (nao importa para MVP)

    // === Campo 3: CurrentHealth ===
    buf.put_u32_le(25000); // 25000 HP (valor tipico do Firefall)

    // === Campo 4: MaxHealth ===
    buf.put_u32_le(25000);

    // === Campo 5: CurrentShields ===
    buf.put_u32_le(12500); // shields

    // === Campo 6: MaxShields ===
    buf.put_u32_le(12500);

    // === Campo 9 (bitfield 2, bit 1): SpawnPose ===
    // SpawnPose = posicao onde o personagem spawna
    // Formato: 3x f32 (x, y, z) + 4x f32 (quaternion rotation)
    buf.put_f32_le(position[0]); // x
    buf.put_f32_le(position[1]); // y
    buf.put_f32_le(position[2]); // z
    // Quaternion (identity = sem rotacao)
    buf.put_f32_le(0.0); // qx
    buf.put_f32_le(0.0); // qy
    buf.put_f32_le(0.0); // qz
    buf.put_f32_le(1.0); // qw

    buf.to_vec()
}

/// Constroi um ObserverView Keyframe minimo (controller_id=8, msg_id=3)
///
/// Contem informacoes visiveis: nome do personagem, genero, raca, visuais.
/// Para MVP, enviamos o minimo para o client mostrar o personagem.
///
/// Baseado em AeroMessages/CharacterObserverView:
/// - Tem nullable bitfields para campos opcionais
/// - StaticInfo com nome, genero, raca
pub fn build_observer_view_keyframe(
    character_name: &str,
    _gender: u8,
    _race: u8,
) -> Vec<u8> {
    let mut buf = BytesMut::with_capacity(256);

    // === Nullable bitfields ===
    // ObserverView campos:
    //   bit 0: StaticInfo (nome, genero, raca, etc)
    //   bit 1: DisplayName (pode ser diferente do nome real)
    //   ... outros campos visuais

    // Bitfield 1: bit 0 = StaticInfo presente
    buf.put_u8(0b0000_0001);

    // Bitfield 2: nenhum campo extra
    buf.put_u8(0b0000_0000);

    // === StaticInfo ===
    // Formato baseado no AeroMessages CharacterStaticInfo:
    // - AeroString name (null-terminated)
    // - Campos adicionais variam por versao

    // Nome do personagem (null-terminated)
    buf.put_slice(character_name.as_bytes());
    buf.put_u8(0x00); // null terminator

    buf.to_vec()
}

/// Constroi um MovementView Keyframe minimo (controller_id=12, msg_id=3)
///
/// Contem posicao, rotacao, velocidade e estado de movimento.
/// Este e o keyframe mais critico para o client renderizar o personagem.
///
/// Baseado em AeroMessages/CharacterMovementView:
/// - Posicao: 3x f32
/// - Rotacao: 4x f32 (quaternion)
/// - Velocidade: 3x f32
/// - MovementState: u16
/// - Campos adicionais via nullable bitfields
pub fn build_movement_view_keyframe(
    position: [f32; 3],
    rotation: [f32; 4],
    velocity: [f32; 3],
    movement_state: u16,
) -> Vec<u8> {
    let mut buf = BytesMut::with_capacity(128);

    // === Nullable bitfields ===
    // MovementView campos:
    //   bit 0: Position + Rotation + Velocity (sempre presentes no keyframe)
    //   bit 1: MovementState
    //   bit 2: AimDirection
    //   ... etc

    // Bitfield 1: bits 0,1 = posicao/rotacao e movementstate
    buf.put_u8(0b0000_0011);

    // Bitfield 2: nenhum campo extra
    buf.put_u8(0b0000_0000);

    // === Campo 0: Posicao + Rotacao + Velocidade ===
    // Posicao (3x f32 LE)
    buf.put_f32_le(position[0]); // x
    buf.put_f32_le(position[1]); // y
    buf.put_f32_le(position[2]); // z

    // Rotacao quaternion (4x f32 LE)
    buf.put_f32_le(rotation[0]); // qx
    buf.put_f32_le(rotation[1]); // qy
    buf.put_f32_le(rotation[2]); // qz
    buf.put_f32_le(rotation[3]); // qw

    // Velocidade (3x f32 LE)
    buf.put_f32_le(velocity[0]); // vx
    buf.put_f32_le(velocity[1]); // vy
    buf.put_f32_le(velocity[2]); // vz

    // === Campo 1: MovementState ===
    // 0x0000 = Idle, 0x0010 = Standing, 0x0020 = Running, etc
    buf.put_u16_le(movement_state);

    buf.to_vec()
}

// ==================== Testes ====================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_gss_header() {
        let entity_id: u64 = 0x0011223344556600;
        let header = build_gss_header(CTRL_CHARACTER_BASE, entity_id, GSS_CONTROLLER_KEYFRAME);

        // byte 0 = controller_id (2)
        assert_eq!(header[0], CTRL_CHARACTER_BASE);
        // bytes 1-7 = entity_id bytes 1-7 (LE)
        let id_bytes = entity_id.to_le_bytes();
        assert_eq!(&header[1..8], &id_bytes[1..8]);
        // byte 8 = message_id (4)
        assert_eq!(header[8], GSS_CONTROLLER_KEYFRAME);
    }

    #[test]
    fn test_entity_id_from_guid() {
        let guid: u64 = 0xABCDEF0123456789;
        let entity_id = entity_id_from_guid(guid);
        // Byte 0 deve ser mascarado para 0
        assert_eq!(entity_id & 0xFF, 0);
        // Bytes 1-7 devem ser preservados
        assert_eq!(entity_id & 0xFFFFFFFF_FFFFFF00, guid & 0xFFFFFFFF_FFFFFF00);
    }

    #[test]
    fn test_build_gss_payload() {
        let payload = build_gss_payload(
            1,                          // seq_num
            CTRL_CHARACTER_BASE,        // controller_id
            0x0000000000001000,         // entity_id
            GSS_CONTROLLER_KEYFRAME,    // msg_id
            &[0xAA, 0xBB],             // data
        );

        // 2 (seq) + 9 (header) + 2 (data) = 13 bytes
        assert_eq!(payload.len(), 13);

        // Seq num Big Endian
        assert_eq!(payload[0], 0x00);
        assert_eq!(payload[1], 0x01);

        // Controller ID
        assert_eq!(payload[2], CTRL_CHARACTER_BASE);

        // Message ID (ultimo byte do header)
        assert_eq!(payload[10], GSS_CONTROLLER_KEYFRAME);

        // Data
        assert_eq!(payload[11], 0xAA);
        assert_eq!(payload[12], 0xBB);
    }

    #[test]
    fn test_build_base_controller_keyframe_not_empty() {
        let data = build_base_controller_keyframe(
            0xFFFF000000000100,
            [297.0, 326.0, 434.0],
        );
        assert!(!data.is_empty());
        // Deve ter pelo menos o player_guid (8) + bitfields (3) + campos
        assert!(data.len() > 8);
    }

    #[test]
    fn test_build_observer_view_keyframe_not_empty() {
        let data = build_observer_view_keyframe("TestPlayer", 1, 0);
        assert!(!data.is_empty());
        // Deve conter o nome null-terminated
        assert!(data.windows(11).any(|w| w == b"TestPlayer\0"));
    }

    #[test]
    fn test_build_movement_view_keyframe_not_empty() {
        let data = build_movement_view_keyframe(
            [297.0, 326.0, 434.0],
            [0.0, 0.0, 0.0, 1.0],
            [0.0, 0.0, 0.0],
            0x0010,
        );
        assert!(!data.is_empty());
        // 2 bitfields + 3*f32 pos + 4*f32 rot + 3*f32 vel + u16 state
        // = 2 + 12 + 16 + 12 + 2 = 44 bytes
        assert_eq!(data.len(), 44);
    }
}

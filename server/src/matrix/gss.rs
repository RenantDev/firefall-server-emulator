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
/// EquipmentView - equipamento do personagem (armas, itens)
pub const CTRL_CHARACTER_EQUIPMENT_VIEW: u8 = 9;
/// CombatView - dados de combate (habilidades, cooldowns)
pub const CTRL_CHARACTER_COMBAT_VIEW: u8 = 11;
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

// ==================== Helpers para serializacao Aero ====================

/// Escreve uma string null-terminated no buffer
fn put_cstring(buf: &mut BytesMut, s: &str) {
    buf.put_slice(s.as_bytes());
    buf.put_u8(0x00);
}

/// Escreve o bloco StaticInfoData comum ao ObserverView e BaseController
/// Contem nome, genero, raca, visuais e metadados do personagem
fn put_static_info(buf: &mut BytesMut, character_name: &str, gender: u8, race: u8) {
    // DisplayName: null-terminated string
    put_cstring(buf, character_name);
    // UniqueName: null-terminated string
    put_cstring(buf, character_name);
    // Gender: u8 (0=male, 1=female)
    buf.put_u8(gender);
    // Race: u8 (0=human)
    buf.put_u8(race);
    // CharInfoId: u32
    buf.put_u32_le(0);
    // HeadMain: u32
    buf.put_u32_le(0);
    // Eyes: u32
    buf.put_u32_le(0);
    // Unk_1: u8
    buf.put_u8(0);
    // TargetFlags: u8
    buf.put_u8(0);
    // StaffFlags: u8
    buf.put_u8(0);
    // CharacterTypeId: u32
    buf.put_u32_le(0);
    // VoiceSet: u32
    buf.put_u32_le(0);
    // TitleId: u16
    buf.put_u16_le(0);
    // NameLocalizationId: u32
    buf.put_u32_le(0);
    // HeadAccessories: byte-prefixed array (vazio)
    buf.put_u8(0);
    // LoadoutVehicle: u32
    buf.put_u32_le(0);
    // LoadoutGlider: u32
    buf.put_u32_le(0);

    // VisualsBlock - todos os arrays vazios (9 bytes)
    buf.put_u8(0); // Decals count
    buf.put_u8(0); // Gradients count
    buf.put_u8(0); // Colors count
    buf.put_u8(0); // Palettes count
    buf.put_u8(0); // Patterns count
    buf.put_u8(0); // OrnamentGroupIds count
    buf.put_u8(0); // CziMapAssetIds count
    buf.put_u8(0); // MorphWeights count
    buf.put_u8(0); // Overlays count

    // ArmyTag: null-terminated string vazia
    put_cstring(buf, "");
}

/// Escreve o bloco CharacterSpawnPose
fn put_spawn_pose(buf: &mut BytesMut, position: [f32; 3]) {
    // Time: u32
    buf.put_u32_le(0);
    // Position: 3x f32
    buf.put_f32_le(position[0]);
    buf.put_f32_le(position[1]);
    buf.put_f32_le(position[2]);
    // Rotation: quaternion (identity)
    buf.put_f32_le(0.0);
    buf.put_f32_le(0.0);
    buf.put_f32_le(0.0);
    buf.put_f32_le(1.0);
    // AimDirection: 3x f32 (forward)
    buf.put_f32_le(1.0);
    buf.put_f32_le(0.0);
    buf.put_f32_le(0.0);
    // Velocity: 3x f32 (parado)
    buf.put_f32_le(0.0);
    buf.put_f32_le(0.0);
    buf.put_f32_le(0.0);
    // MovementState: u16 (Standing)
    buf.put_u16_le(0x0010);
    // Unk1: u8
    buf.put_u8(0);
    // Unk2: u8
    buf.put_u8(0);
    // JetpackEnergy: u16
    buf.put_u16_le(10000);
    // AirGroundTimer: i16
    buf.put_i16_le(0);
    // JumpTimer: i16
    buf.put_i16_le(0);
    // HaveDebugData: u8
    buf.put_u8(0);
}

/// Escreve o bloco CharacterStatsData (todos vazios)
fn put_character_stats(buf: &mut BytesMut) {
    // ItemAttributes count: u16
    buf.put_u16_le(0);
    // Unk1: u32
    buf.put_u32_le(0);
    // WeaponA count: u16
    buf.put_u16_le(0);
    // Unk2: u32
    buf.put_u32_le(0);
    // WeaponB count: u16
    buf.put_u16_le(0);
    // Unk3: u32
    buf.put_u32_le(0);
    // AttributeCategories1 count: u16
    buf.put_u16_le(0);
    // AttributeCategories2 count: u16
    buf.put_u16_le(0);
}

// ==================== EquipmentView Keyframe ====================

/// Constroi um EquipmentView Keyframe (controller_id=9, msg_id=3)
///
/// EquipmentView tem campos nullable. Para MVP, enviamos bitfield com
/// todos os campos ausentes (todos bits = 0x00 seria presente, FF = ausente).
/// Formato minimo: bitfield indicando que nao ha equipamento.
pub fn build_equipment_view_keyframe() -> Vec<u8> {
    let mut buf = BytesMut::with_capacity(32);
    // Bitfield: 4 bytes com todos os campos nullable ausentes (bits=1 = ausente)
    buf.put_u8(0xFF);
    buf.put_u8(0xFF);
    buf.put_u8(0xFF);
    buf.put_u8(0xFF);
    buf.to_vec()
}

// ==================== CombatView Keyframe ====================

/// Constroi um CombatView Keyframe (controller_id=11, msg_id=3)
///
/// CombatView tem campos nullable para habilidades, cooldowns etc.
/// Para MVP, enviamos bitfield com todos os campos ausentes.
pub fn build_combat_view_keyframe() -> Vec<u8> {
    let mut buf = BytesMut::with_capacity(32);
    // Bitfield: 4 bytes com todos os campos nullable ausentes
    buf.put_u8(0xFF);
    buf.put_u8(0xFF);
    buf.put_u8(0xFF);
    buf.put_u8(0xFF);
    buf.to_vec()
}

// ==================== MovementView Keyframe ====================

/// Constroi um MovementView Keyframe (controller_id=12, msg_id=3)
///
/// MovementView tem 0 campos nullable, portanto NAO tem bitfield prefix.
/// Apenas 1 campo (MovementData) que e sempre serializado:
///   Position (3x f32) + Rotation (4x f32) + Aim (3x f32) +
///   MovementState (u16) + Time (u32) = 46 bytes total
pub fn build_movement_view_keyframe(
    position: [f32; 3],
    rotation: [f32; 4],
    aim_direction: [f32; 3],
    movement_state: u16,
) -> Vec<u8> {
    let mut buf = BytesMut::with_capacity(46);

    // SEM bitfield - MovementView tem 0 campos nullable

    // Position: 3x f32 LE
    buf.put_f32_le(position[0]);
    buf.put_f32_le(position[1]);
    buf.put_f32_le(position[2]);

    // Rotation: quaternion 4x f32 LE
    buf.put_f32_le(rotation[0]);
    buf.put_f32_le(rotation[1]);
    buf.put_f32_le(rotation[2]);
    buf.put_f32_le(rotation[3]);

    // AimDirection: 3x f32 LE
    buf.put_f32_le(aim_direction[0]);
    buf.put_f32_le(aim_direction[1]);
    buf.put_f32_le(aim_direction[2]);

    // MovementState: u16 LE (0x0010 = Standing)
    buf.put_u16_le(movement_state);

    // Time: u32 LE (0 = inicio)
    buf.put_u32_le(0);

    buf.to_vec()
}

// ==================== ObserverView Keyframe ====================

/// Constroi um ObserverView Keyframe (controller_id=8, msg_id=3)
///
/// ObserverView tem 32 campos nullable = 4 bytes de bitfield.
/// Estrategia MVP: todos os bits nullable = 0 (ausentes).
/// Serializa apenas os campos non-nullable com valores default.
pub fn build_observer_view_keyframe(
    character_name: &str,
    gender: u8,
    race: u8,
) -> Vec<u8> {
    let mut buf = BytesMut::with_capacity(512);

    // === 4 bytes de nullable bitfield (32 campos nullable, todos ausentes) ===
    buf.put_u8(0x00);
    buf.put_u8(0x00);
    buf.put_u8(0x00);
    buf.put_u8(0x00);

    // === Campos non-nullable em ordem de declaracao ===

    // 1. StaticInfo: StaticInfoData
    put_static_info(&mut buf, character_name, gender, race);

    // 2. SpawnTime: u32
    buf.put_u32_le(0);

    // 3. EffectsFlags: u8
    buf.put_u8(0);

    // 4. GibVisualsID: GibVisuals = {u32, u32}
    buf.put_u32_le(0);
    buf.put_u32_le(0);

    // 5. ProcessDelay: {u16, u16}
    buf.put_u16_le(0);
    buf.put_u16_le(0);

    // 6. CharacterState: {u8 state, u32 time}
    buf.put_u8(6); // Living
    buf.put_u32_le(0);

    // 7. HostilityInfo: u8(0) - flags=0, sem campos condicionais
    buf.put_u8(0);

    // (NULLABLE 0: PersonalFactionStance - ausente)

    // 8. CurrentHealthPct: u8
    buf.put_u8(100);

    // 9. MaxHealth: {i32 value, u32 time}
    buf.put_i32_le(25000);
    buf.put_u32_le(0);

    // 10. EmoteID: EmoteData = {u32, f32}
    buf.put_u32_le(0);
    buf.put_f32_le(0.0);

    // (NULLABLE 1: AttachedTo - ausente)

    // 11. SnapMount: u8
    buf.put_u8(0);

    // 12. SinFlags: u8
    buf.put_u8(0);

    // (NULLABLE 2,3: ausentes)

    // 13. ArmyGUID: u64
    buf.put_u64_le(0);

    // 14. OwnerId: u64
    buf.put_u64_le(0);

    // 15. NPCType: u16
    buf.put_u16_le(0);

    // 16. DockedParams: DockedParamsData = {u32, u8, u64}
    buf.put_u32_le(0);
    buf.put_u8(0);
    buf.put_u64_le(0);

    // (NULLABLE 4: LookAtTarget - ausente)

    // 17. WaterLevelAndDesc: u8
    buf.put_u8(0);

    // (NULLABLE 5-8: ausentes)

    // 18. SinCardType: u32
    buf.put_u32_le(0);

    // (NULLABLE 9-31: ausentes)

    // 19. AssetOverrides: byte-prefixed array (vazio)
    buf.put_u8(0);

    buf.to_vec()
}

// ==================== BaseController Keyframe ====================

/// Constroi um BaseController Keyframe (controller_id=2, msg_id=4)
///
/// BaseController tem 38 campos nullable = 5 bytes de bitfield.
/// Controller keyframes tem prefixo de 8 bytes com o player GUID.
/// Estrategia MVP: todos os bits nullable = 0 (ausentes).
/// Serializa TODOS os campos non-nullable com valores default.
pub fn build_base_controller_keyframe(
    character_guid: u64,
    position: [f32; 3],
) -> Vec<u8> {
    let mut buf = BytesMut::with_capacity(1024);

    // === Prefixo: player_guid (8 bytes LE) ===
    buf.put_u64_le(character_guid);

    // === 5 bytes de nullable bitfield (38 campos nullable, todos ausentes) ===
    buf.put_u8(0x00);
    buf.put_u8(0x00);
    buf.put_u8(0x00);
    buf.put_u8(0x00);
    buf.put_u8(0x00);

    // === Campos non-nullable em ordem de declaracao ===

    // 1. TimePlayed: i32
    buf.put_i32_le(0);

    // 2. CurrentWeight: i32
    buf.put_i32_le(0);

    // 3. EncumberedWeight: i32
    buf.put_i32_le(1000);

    // 4. AuthorizedTerminal: {u8, u32, u64}
    buf.put_u8(0);
    buf.put_u32_le(0);
    buf.put_u64_le(0);

    // 5. PingTime: u32
    buf.put_u32_le(0);

    // 6. StaticInfo: StaticInfoData (mesmo formato do ObserverView)
    put_static_info(&mut buf, "Player", 0, 0);

    // 7. SpawnTime: u32
    buf.put_u32_le(0);

    // 8. VisualOverrides: byte-prefixed array (vazio)
    buf.put_u8(0);

    // 9. CurrentEquipment: EquipmentData (minimo/vazio)
    //    Chassis SlottedItem (vazio): u32(0)
    buf.put_u32_le(0);
    //    Backpack SlottedItem (vazio): u32(0)
    buf.put_u32_le(0);
    //    PrimaryWeapon SlottedItem (vazio): u32(0)
    buf.put_u32_le(0);
    //    SecondaryWeapon SlottedItem (vazio): u32(0)
    buf.put_u32_le(0);
    //    EndUnk1: u32
    buf.put_u32_le(0);
    //    EndUnk2: u32
    buf.put_u32_le(0);

    // 10. SelectedLoadout: i32
    buf.put_i32_le(0);

    // 11. SelectedLoadoutIsPvP: u32
    buf.put_u32_le(0);

    // 12. GibVisualsId: GibVisuals = {u32, u32}
    buf.put_u32_le(0);
    buf.put_u32_le(0);

    // 13. SpawnPose: CharacterSpawnPose
    put_spawn_pose(&mut buf, position);

    // 14. ProcessDelay: {u16, u16}
    buf.put_u16_le(0);
    buf.put_u16_le(0);

    // 15. SpectatorMode: u8
    buf.put_u8(0);

    // (NULLABLE 0: CinematicCamera - ausente)

    // 16. CharacterState: {u8 state=Living, u32 time=0}
    buf.put_u8(6); // Living
    buf.put_u32_le(0);

    // 17. HostilityInfo: u8(0) - flags=0, sem campos condicionais
    buf.put_u8(0);

    // (NULLABLE 1: PersonalFactionStance - ausente)

    // 18. CurrentHealth: i32
    buf.put_i32_le(25000);

    // 19. CurrentShields: i32
    buf.put_i32_le(12500);

    // 20. MaxShields: {i32, u32}
    buf.put_i32_le(12500);
    buf.put_u32_le(0);

    // 21. MaxHealth: {i32, u32}
    buf.put_i32_le(25000);
    buf.put_u32_le(0);

    // 22. CurrentDurabilityPct: u8
    buf.put_u8(100);

    // 23. EnergyParams: {f32 current, u32 time, f32 max, u32 time}
    buf.put_f32_le(100.0);
    buf.put_u32_le(0);
    buf.put_f32_le(100.0);
    buf.put_u32_le(0);

    // 24. CharacterStats: todos vazios
    put_character_stats(&mut buf);

    // 25. EmoteID: {u32, f32}
    buf.put_u32_le(0);
    buf.put_f32_le(0.0);

    // (NULLABLE 2: AttachedTo - ausente)

    // 26. SnapMount: u8
    buf.put_u8(0);

    // 27. SinFlags: u8
    buf.put_u8(0);

    // 28. SinFlagsPrivate: u8
    buf.put_u8(0);

    // (NULLABLE 3,4: ausentes)

    // 29. ArmyGUID: u64
    buf.put_u64_le(0);

    // 30. ArmyIsOfficer: i8
    buf.put_i8(0);

    // (NULLABLE 5: ausente)

    // 31. DockedParams: {u32, u8, u64}
    buf.put_u32_le(0);
    buf.put_u8(0);
    buf.put_u64_le(0);

    // (NULLABLE 6: ausente)

    // 32. ZoneUnlocks: u64
    buf.put_u64_le(0);

    // 33. RegionUnlocks: u64
    buf.put_u64_le(0);

    // 34. ChatPartyLeaderId: u64
    buf.put_u64_le(0);

    // 35. ScopeBubbleInfo: {u32, u32}
    buf.put_u32_le(0);
    buf.put_u32_le(0);

    // (NULLABLE 7-11: ausentes)

    // 36. ProgressionXp: u32
    buf.put_u32_le(0);

    // 37. PermanentStatusEffects: byte-prefixed array (vazio)
    buf.put_u8(0);

    // 38-57. 20x StatModifier: {u32 id, f32 value} = 8 bytes cada, 160 total
    for _ in 0..20 {
        buf.put_u32_le(0);
        buf.put_f32_le(0.0);
    }

    // 58. Wallet: {u32, u32}
    buf.put_u32_le(0);
    buf.put_u32_le(0);

    // 59. Loyalty: {u32, u32, u32}
    buf.put_u32_le(0);
    buf.put_u32_le(0);
    buf.put_u32_le(0);

    // 60. Level: u8
    buf.put_u8(1);

    // 61. EffectiveLevel: u8
    buf.put_u8(1);

    // 62. LevelResetCount: u8
    buf.put_u8(0);

    // 63. OldestDeployables: byte-prefixed array (vazio)
    buf.put_u8(0);

    // 64. PerkRespecs: u32
    buf.put_u32_le(0);

    // (NULLABLE 12,13: ausentes)

    // 65. ChatMuteStatus: u8
    buf.put_u8(0);

    // 66. TimedDailyReward: {u8, u8, u8, u8, u32}
    buf.put_u8(0);
    buf.put_u8(0);
    buf.put_u8(0);
    buf.put_u8(0);
    buf.put_u32_le(0);

    // (NULLABLE 14: ausente)

    // 67. SinCardType: u32
    buf.put_u32_le(0);

    // (NULLABLE 15-37: ausentes)

    // 68. AssetOverrides: byte-prefixed array (vazio)
    buf.put_u8(0);

    // 69. FriendCount: u16
    buf.put_u16_le(0);

    // 70. CAISStatus: {u8, u32}
    buf.put_u8(0);
    buf.put_u32_le(0);

    // 71. ScalingLevel: u32
    buf.put_u32_le(0);

    // 72. PvPRank: u32
    buf.put_u32_le(0);

    // 73. PvPRankPoints: u32
    buf.put_u32_le(0);

    // 74. PvPTokens: u32
    buf.put_u32_le(0);

    // 75. BountyPointsLastClaimed: u32
    buf.put_u32_le(0);

    // 76. EliteLevel: u32
    buf.put_u32_le(0);

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
    fn test_movement_view_keyframe_format() {
        let data = build_movement_view_keyframe(
            [297.0, 326.0, 434.0],
            [0.0, 0.0, 0.0, 1.0],
            [1.0, 0.0, 0.0],
            0x0010,
        );

        // MovementView: 0 campos nullable = SEM bitfield
        // Position(12) + Rotation(16) + Aim(12) + MovementState(2) + Time(4) = 46 bytes
        assert_eq!(data.len(), 46);

        // Verificar posicao X no inicio (f32 LE)
        let x = f32::from_le_bytes([data[0], data[1], data[2], data[3]]);
        assert_eq!(x, 297.0);

        // Verificar rotation W (offset 12+12=24, 4th float at offset 24)
        let qw = f32::from_le_bytes([data[24], data[25], data[26], data[27]]);
        assert_eq!(qw, 1.0);

        // Verificar aim X (offset 28)
        let aim_x = f32::from_le_bytes([data[28], data[29], data[30], data[31]]);
        assert_eq!(aim_x, 1.0);

        // Verificar MovementState (offset 40)
        let state = u16::from_le_bytes([data[40], data[41]]);
        assert_eq!(state, 0x0010);

        // Verificar Time (offset 42)
        let time = u32::from_le_bytes([data[42], data[43], data[44], data[45]]);
        assert_eq!(time, 0);
    }

    #[test]
    fn test_observer_view_keyframe_format() {
        let data = build_observer_view_keyframe("TestPlayer", 0, 0);
        assert!(!data.is_empty());

        // Primeiros 4 bytes devem ser bitfield = 0x00 0x00 0x00 0x00
        assert_eq!(&data[0..4], &[0x00, 0x00, 0x00, 0x00]);

        // StaticInfo comeca no offset 4 com DisplayName null-terminated
        let name_end = 4 + "TestPlayer".len();
        assert_eq!(&data[4..name_end], b"TestPlayer");
        assert_eq!(data[name_end], 0x00); // null terminator

        // Deve conter CharacterState = Living (6) em algum lugar
        // e CurrentHealthPct = 100
        assert!(data.len() > 50, "ObserverView deve ter tamanho razoavel, tem {} bytes", data.len());
    }

    #[test]
    fn test_base_controller_keyframe_format() {
        let guid: u64 = 0xFFFF000000000100;
        let data = build_base_controller_keyframe(guid, [297.0, 326.0, 434.0]);

        // Primeiros 8 bytes = player_guid LE
        let stored_guid = u64::from_le_bytes([
            data[0], data[1], data[2], data[3],
            data[4], data[5], data[6], data[7],
        ]);
        assert_eq!(stored_guid, guid);

        // Proximos 5 bytes = bitfield (todos 0)
        assert_eq!(&data[8..13], &[0x00, 0x00, 0x00, 0x00, 0x00]);

        // Deve ter tamanho substancial (muitos campos non-nullable)
        assert!(data.len() > 200, "BaseController deve ter tamanho grande, tem {} bytes", data.len());
    }

    #[test]
    fn test_base_controller_contains_spawn_pose() {
        let data = build_base_controller_keyframe(0x100, [297.0, 326.0, 434.0]);

        // Verificar que a posicao 297.0 aparece nos dados (como f32 LE)
        let pos_x_bytes = 297.0_f32.to_le_bytes();
        let found = data.windows(4).any(|w| w == pos_x_bytes);
        assert!(found, "SpawnPose deve conter a posicao X=297.0");
    }

    #[test]
    fn test_base_controller_contains_health() {
        let data = build_base_controller_keyframe(0x100, [0.0, 0.0, 0.0]);

        // Verificar que 25000 (health) aparece como i32 LE
        let health_bytes = 25000_i32.to_le_bytes();
        let count = data.windows(4).filter(|w| *w == health_bytes).count();
        // Deve aparecer pelo menos 2x (CurrentHealth e MaxHealth.value)
        assert!(count >= 2, "Deve conter health=25000 pelo menos 2x, encontrou {}x", count);
    }
}

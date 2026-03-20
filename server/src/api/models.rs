use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

// --- Account ---

#[derive(Debug, Serialize, Deserialize, sqlx::FromRow)]
pub struct Account {
    pub id: Uuid,
    pub email: String,
    #[serde(skip_serializing)]
    pub password_hash: String,
    pub display_name: String,
    pub is_vip: bool,
    pub character_limit: i32,
    pub language: String,
    pub steam_id: Option<String>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Deserialize)]
pub struct LoginRequest {
    pub email: String,
    pub password: String,
}

#[derive(Debug, Serialize)]
pub struct LoginResponse {
    pub success: bool,
    pub account_id: Option<Uuid>,
    pub session_token: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    pub is_vip: bool,
    pub character_limit: i32,
}

#[derive(Debug, Serialize)]
pub struct AccountInfo {
    pub account_id: Uuid,
    pub email: String,
    pub display_name: String,
    pub is_vip: bool,
    pub character_limit: i32,
    pub created_at: DateTime<Utc>,
}

// --- Character ---

#[derive(Debug, Serialize, Deserialize, sqlx::FromRow)]
pub struct Character {
    pub character_guid: Uuid,
    pub account_id: Uuid,
    pub name: String,
    pub unique_name: String,
    pub is_active: bool,
    pub is_dev: bool,
    pub gender: i32,
    pub race: String,
    pub title_id: i32,
    pub time_played_secs: i64,
    pub created_at: DateTime<Utc>,
    pub deleted_at: Option<DateTime<Utc>>,
    pub head: i32,
    pub eye_color: i32,
    pub lip_color: i32,
    pub hair_color: i32,
    pub facial_hair_color: i32,
    pub skin_color: i32,
    pub voice_set: i32,
    pub level: i32,
    pub current_battleframe: String,
    pub frame_sdb_id: i32,
}

/// Request de criacao de personagem - formato que o client envia
/// O client envia: {"skin_color_id":77177,"start_class_id":75773,"eye_color_id":77185,
///   "name":"Mutar","environment":"prod","voice_set":1001,"head":10032,
///   "gender":"male","hair_color_id":77189,"is_dev":false,"head_accessory_a":10091}
#[derive(Debug, Deserialize)]
pub struct CreateCharacterRequest {
    pub name: String,
    #[serde(default)]
    pub start_class_id: Option<i64>,
    #[serde(default)]
    pub is_dev: Option<bool>,
    #[serde(default)]
    pub gender: Option<String>,
    #[serde(default)]
    pub environment: Option<String>,
    #[serde(default)]
    pub head: Option<i32>,
    #[serde(default)]
    pub head_accessory_a: Option<i32>,
    #[serde(default)]
    pub head_accessory_b: Option<i32>,
    #[serde(default)]
    pub eye_color_id: Option<i32>,
    #[serde(default)]
    pub hair_color_id: Option<i32>,
    #[serde(default)]
    pub skin_color_id: Option<i32>,
    #[serde(default)]
    pub voice_set: Option<serde_json::Value>,
    // Campos legados (manter compatibilidade)
    #[serde(default)]
    pub race: Option<String>,
    #[serde(default)]
    pub lip_color: Option<i32>,
    #[serde(default)]
    pub facial_hair_color: Option<i32>,
}

#[derive(Debug, Deserialize)]
pub struct ValidateNameRequest {
    pub name: String,
    #[serde(default)]
    pub lang: Option<String>,
}

/// Resposta de validacao de nome - formato que o client espera
#[derive(Debug, Serialize)]
pub struct ValidateNameResponse {
    pub valid: bool,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<Vec<String>>,
}

/// Resposta da lista de personagens - formato que o client espera
#[derive(Debug, Serialize)]
pub struct CharacterListResponse {
    pub is_dev: bool,
    pub rb_balance: i64,
    pub name_change_cost: i32,
    pub characters: Vec<CharacterInfo>,
}

/// Info de um personagem - formato que o client espera
/// NOTA: character_guid DEVE ser um number (i64), nao uma string UUID
#[derive(Debug, Serialize)]
pub struct CharacterInfo {
    pub character_guid: i64,
    pub name: String,
    pub unique_name: String,
    pub is_dev: bool,
    pub is_active: bool,
    pub created_at: String,
    pub title_id: i32,
    pub time_played_secs: i64,
    pub needs_name_change: bool,
    pub max_frame_level: i32,
    pub frame_sdb_id: i32,
    pub current_level: i32,
    pub gender: i32,
    pub current_gender: String,
    pub elite_rank: i32,
    pub last_seen_at: String,
    pub visuals: serde_json::Value,
    pub gear: Vec<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expires_in: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub deleted_at: Option<String>,
    pub race: String,
    pub migrations: Vec<serde_json::Value>,
}

// --- Server/Zone ---

/// Resposta da lista de servidores - formato que o client espera
#[derive(Debug, Serialize)]
pub struct ServerListResponse {
    pub zone_list: Vec<ZoneEntry>,
}

/// Entrada de zona na lista de servidores
#[derive(Debug, Serialize)]
pub struct ZoneEntry {
    pub zone_name: String,
    pub matrix_url: String,
    pub owner: String,
    pub players: i32,
    #[serde(rename = "match")]
    pub match_id: i32,
    pub revision: i32,
    pub protocol_version: i64,
}

#[derive(Debug, Serialize)]
pub struct LoginAlert {
    pub message: String,
    pub severity: String,
}

// --- Zone Settings ---

#[derive(Debug, Serialize)]
pub struct ZoneSettings {
    pub zone_id: i32,
    pub zone_name: String,
    pub context: String,
    pub gametype: String,
    pub max_players: i32,
}

// --- Oracle Ticket ---

/// Resposta do oracle/ticket - formato que o engine C++ espera
#[derive(Debug, Serialize)]
pub struct OracleTicketResponse {
    pub country: String,
    pub datacenter: String,
    pub hostname: String,
    pub matrix_url: String,
    pub operator_override: serde_json::Value,
    pub session_id: String,
    pub ticket: String,
}

// --- Account Creation ---

#[derive(Debug, Deserialize)]
pub struct CreateAccountRequest {
    pub email: String,
    pub password: String,
    pub country: Option<String>,
    pub birthday: Option<String>,
    pub referral_key: Option<String>,
    pub email_consent: Option<bool>,
}

#[derive(Debug, Serialize)]
pub struct CreateAccountResponse {
    pub success: bool,
    pub account_id: Option<Uuid>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

// --- Generic ---

#[derive(Debug, Serialize)]
pub struct GenericResponse {
    pub success: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

// --- Garage ---

#[derive(Debug, Serialize)]
pub struct GarageSlot {
    pub slot_id: i32,
    pub battleframe: String,
    pub level: i32,
    pub xp: i64,
}

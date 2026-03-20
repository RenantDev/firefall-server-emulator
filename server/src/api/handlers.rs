use std::sync::Arc;

use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::Json;
use serde::Deserialize;
use sha2::{Digest, Sha256};
use uuid::Uuid;

use super::models::*;
use crate::AppState;

type AppResult<T> = Result<Json<T>, (StatusCode, Json<GenericResponse>)>;

fn err(status: StatusCode, msg: &str) -> (StatusCode, Json<GenericResponse>) {
    (
        status,
        Json(GenericResponse {
            success: false,
            error: Some(msg.to_string()),
        }),
    )
}

fn hash_password(password: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(password.as_bytes());
    hex::encode(hasher.finalize())
}

/// Converte UUID para um i64 deterministico (usa os primeiros 8 bytes)
/// O client Firefall espera character_guid como number, nao string UUID
fn uuid_to_i64(uuid: &Uuid) -> i64 {
    let bytes = uuid.as_bytes();
    let mut arr = [0u8; 8];
    arr.copy_from_slice(&bytes[..8]);
    // Garante positivo usando abs, mas evita overflow
    let val = i64::from_be_bytes(arr);
    val.unsigned_abs() as i64
}

// ============================================================================
// Auth
// ============================================================================

/// POST /api/v2/accounts/login
/// Chamado nativamente pelo engine C++ via System.RequestLogin
/// O engine pode enviar body vazio (content-length: 0), entao NAO fazemos parse do body.
/// Resposta deve conter can_login, is_dev, steam_auth_prompt, etc.
pub async fn native_login(
    headers: axum::http::HeaderMap,
    body: axum::body::Bytes,
) -> axum::response::Response {
    tracing::info!("=== POST /api/v2/accounts/login ===");
    for (name, value) in headers.iter() {
        tracing::info!("  Header: {}: {:?}", name, value);
    }
    if let Ok(body_str) = std::str::from_utf8(&body) {
        if !body_str.is_empty() {
            tracing::info!("  Body: {}", body_str);
        } else {
            tracing::info!("  Body: (empty)");
        }
    } else {
        tracing::info!("  Body: ({} bytes, binary)", body.len());
    }

    // IMPORTANTE: O parser JSON C++ do engine (slserialize_json.h) e SEQUENCIAL
    // e espera as chaves em uma ORDEM ESPECIFICA. serde_json ordena alfabeticamente
    // o que quebra o parser. Usar string literal com ordem correta.
    // Ordem EXATA do RIN.WebAPI LoginResp.cs (C# serializa na ordem de declaracao):
    // account_id, can_login, events, is_dev, steam_auth_prompt, skip_precursor,
    // cais_status{state,duration,expires_at}, created_at, character_limit, is_vip, vip_expiration
    //
    // is_dev=true para forcar o Lua a usar matrix_url da zone_list diretamente
    // (em vez de depender do sistema Oracle interno do engine C++)
    let response = r#"{"account_id":12345,"can_login":true,"events":{"count":0,"results":[]},"is_dev":true,"steam_auth_prompt":false,"skip_precursor":true,"cais_status":{"state":"disabled","duration":0,"expires_at":0},"created_at":1426636800,"character_limit":4,"is_vip":true,"vip_expiration":0}"#;

    tracing::info!("  Response: {}", response);

    axum::response::Response::builder()
        .status(200)
        .header("Content-Type", "application/json")
        .body(axum::body::Body::from(response))
        .unwrap()
}

/// POST /api/v1/auth/login - Login customizado do emulador
pub async fn login(
    State(state): State<Arc<AppState>>,
    Json(req): Json<LoginRequest>,
) -> AppResult<LoginResponse> {
    let account = sqlx::query_as::<_, Account>("SELECT * FROM accounts WHERE email = $1")
        .bind(&req.email)
        .fetch_optional(&state.db)
        .await
        .map_err(|_| err(StatusCode::INTERNAL_SERVER_ERROR, "Database error"))?;

    let Some(account) = account else {
        return Ok(Json(LoginResponse {
            success: false,
            account_id: None,
            session_token: None,
            error: Some("Invalid credentials".into()),
            is_vip: false,
            character_limit: 2,
        }));
    };

    let input_hash = hash_password(&req.password);
    if input_hash != account.password_hash {
        return Ok(Json(LoginResponse {
            success: false,
            account_id: None,
            session_token: None,
            error: Some("Invalid credentials".into()),
            is_vip: false,
            character_limit: 2,
        }));
    }

    let session_token = Uuid::new_v4().to_string();

    Ok(Json(LoginResponse {
        success: true,
        account_id: Some(account.id),
        session_token: Some(session_token),
        error: None,
        is_vip: account.is_vip,
        character_limit: account.character_limit,
    }))
}

/// POST /api/v2/accounts - Info da conta
pub async fn get_account_info(
    headers: axum::http::HeaderMap,
    body: axum::body::Bytes,
) -> axum::response::Response {
    tracing::info!("=== POST /api/v2/accounts ===");
    for (name, value) in headers.iter() {
        tracing::info!("  Header: {}: {:?}", name, value);
    }
    if let Ok(body_str) = std::str::from_utf8(&body) {
        if !body_str.is_empty() {
            tracing::info!("  Body: {}", body_str);
        }
    }

    let response = serde_json::json!({
        "account_id": 12345,
        "is_dev": false,
        "is_vip": true,
        "vip_expiration": 0,
        "character_limit": 4,
        "created_at": 1609459200
    });

    axum::response::Response::builder()
        .status(200)
        .header("Content-Type", "application/json")
        .body(axum::body::Body::from(response.to_string()))
        .unwrap()
}

#[derive(Deserialize)]
pub struct TotpQuery {
    pub totp: Option<String>,
}

pub async fn get_cookie_totp(
    Query(_query): Query<TotpQuery>,
) -> AppResult<GenericResponse> {
    tracing::info!("=== GET /api/v2/accounts/get_cookie ===");
    Ok(Json(GenericResponse {
        success: true,
        error: None,
    }))
}

pub async fn email_totp() -> AppResult<GenericResponse> {
    tracing::info!("=== POST /api/v2/accounts/email_totp ===");
    Ok(Json(GenericResponse {
        success: true,
        error: None,
    }))
}

pub async fn link_steam_account() -> AppResult<GenericResponse> {
    tracing::info!("=== POST /api/v2/accounts/link_steam_account ===");
    Ok(Json(GenericResponse {
        success: true,
        error: None,
    }))
}

/// GET /api/v2/accounts/character_slots
/// Retorna array vazio - o client espera um JSON array
pub async fn character_slots() -> axum::response::Response {
    tracing::info!("=== GET/POST /api/v2/accounts/character_slots ===");
    axum::response::Response::builder()
        .status(200)
        .header("Content-Type", "application/json")
        .body(axum::body::Body::from("[]"))
        .unwrap()
}

pub async fn change_language(
    body: axum::body::Bytes,
) -> AppResult<GenericResponse> {
    tracing::info!("=== POST /api/v2/accounts/change_language ===");
    if let Ok(body_str) = std::str::from_utf8(&body) {
        tracing::info!("  Body: {}", body_str);
    }
    Ok(Json(GenericResponse {
        success: true,
        error: None,
    }))
}

// ============================================================================
// Oracle Ticket
// ============================================================================

/// POST /api/v1/oracle/ticket
/// Endpoint CRITICO chamado nativamente pelo engine C++ para obter ticket de conexao ao Matrix.
/// O ticket deve ser uma string base64 de pelo menos 55 bytes.
pub async fn oracle_ticket(
    headers: axum::http::HeaderMap,
    body: axum::body::Bytes,
) -> axum::response::Response {
    tracing::info!("=== POST /api/v1/oracle/ticket ===");
    for (name, value) in headers.iter() {
        tracing::info!("  Header: {}: {:?}", name, value);
    }
    if let Ok(body_str) = std::str::from_utf8(&body) {
        if !body_str.is_empty() {
            tracing::info!("  Body: {}", body_str);
        }
    }

    let matrix_host = std::env::var("PUBLIC_MATRIX_HOST").unwrap_or_else(|_| "127.0.0.1".into());
    let matrix_port = std::env::var("MATRIX_PORT").unwrap_or_else(|_| "25000".into());
    let session_id = Uuid::new_v4().to_string();

    // Ticket dummy de 76 bytes em base64 (>55 bytes conforme exigido)
    let dummy_ticket = "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA";

    let response = serde_json::json!({
        "country": "US",
        "datacenter": "local",
        "hostname": "localhost",
        "matrix_url": format!("{}:{}", matrix_host, matrix_port),
        "operator_override": {},
        "session_id": session_id,
        "ticket": dummy_ticket
    });

    tracing::info!("  Oracle ticket response: {}", response);

    axum::response::Response::builder()
        .status(200)
        .header("Content-Type", "application/json")
        .body(axum::body::Body::from(response.to_string()))
        .unwrap()
}

// ============================================================================
// Characters
// ============================================================================

/// GET /api/v2/characters/list
/// Retorna lista de personagens no formato que o client espera.
/// NOTA: character_guid deve ser um NUMBER (i64), nao string UUID.
pub async fn list_characters(
    State(state): State<Arc<AppState>>,
) -> axum::response::Response {
    tracing::info!("=== GET /api/v2/characters/list ===");

    let characters = sqlx::query_as::<_, Character>(
        "SELECT * FROM characters WHERE deleted_at IS NULL ORDER BY created_at",
    )
    .fetch_all(&state.db)
    .await;

    let chars = match characters {
        Ok(chars) => chars,
        Err(e) => {
            tracing::error!("Database error listing characters: {}", e);
            // Retorna lista vazia em caso de erro (nao travar o client)
            vec![]
        }
    };

    let character_list: Vec<serde_json::Value> = chars
        .into_iter()
        .map(|c| {
            let guid_num = uuid_to_i64(&c.character_guid);
            let gender_str = if c.gender == 0 { "male" } else { "female" };

            // Construir visuals com os dados salvos do personagem
            // IDs baseados no que o client envia na criacao e nos defaults do PIN
            let head_id = if c.head > 0 { c.head } else { 10002 };
            let eye_color_id = if c.eye_color > 0 { c.eye_color } else { 77184 };
            let skin_color_id = if c.skin_color > 0 { c.skin_color } else { 77179 };
            let hair_color_id = if c.hair_color > 0 { c.hair_color } else { 77189 };

            let visuals = serde_json::json!({
                "head": {"id": head_id},
                "eyes": {"id": eye_color_id},
                "skin_color": {
                    "id": skin_color_id,
                    "value": {"color": "#A0785A"}
                },
                "eye_color": {
                    "id": eye_color_id,
                    "value": {"color": "#4488AA"}
                },
                "hair_color": {
                    "id": hair_color_id,
                    "value": {"color": "#2C1A0E"}
                },
                "lip_color": {
                    "id": 0,
                    "value": {"color": "#884444"}
                },
                "facial_hair_color": {
                    "id": 0,
                    "value": {"color": "#2C1A0E"}
                },
                "head_accessories": [
                    {"id": 10089},
                    {"id": 0}
                ],
                "ornaments": [
                    {"id": 0}, {"id": 0}, {"id": 0}, {"id": 0}, {"id": 0}
                ],
                "warpaint": [
                    "#1a1a1a", "#1a1a1a", "#1a1a1a",
                    "#1a1a1a", "#1a1a1a",
                    "#40FFC4", "#40FFC4"
                ],
                "warpaint_patterns": [],
                "decals": []
            });

            serde_json::json!({
                "character_guid": guid_num,
                "name": c.name,
                "unique_name": c.unique_name,
                "is_dev": c.is_dev,
                "is_active": c.is_active,
                "created_at": c.created_at.to_rfc3339(),
                "title_id": c.title_id,
                "time_played_secs": c.time_played_secs,
                "needs_name_change": false,
                "max_frame_level": 40,
                "frame_sdb_id": c.frame_sdb_id,
                "current_level": c.level,
                "gender": c.gender,
                "current_gender": gender_str,
                "elite_rank": 0,
                "last_seen_at": c.created_at.to_rfc3339(),
                "visuals": visuals,
                "gear": [],
                "expires_in": null,
                "deleted_at": null,
                "race": c.race,
                "migrations": []
            })
        })
        .collect();

    let response = serde_json::json!({
        "is_dev": false,
        "rb_balance": 0,
        "name_change_cost": 100,
        "characters": character_list
    });

    tracing::info!("  Characters response: {} characters", character_list.len());

    axum::response::Response::builder()
        .status(200)
        .header("Content-Type", "application/json")
        .body(axum::body::Body::from(response.to_string()))
        .unwrap()
}

/// POST /api/v1/characters - Criar personagem
/// Request body do client:
/// {"name":"TestPlayer","start_class_id":76164,"is_dev":false,"gender":"male",
///  "head":10002,"head_accessory_a":10089,"eye_color_id":77184,
///  "hair_color_id":77189,"skin_color_id":77179,"voice_set":"voicePrint"}
pub async fn create_character(
    State(state): State<Arc<AppState>>,
    body: axum::body::Bytes,
) -> axum::response::Response {
    tracing::info!("=== POST /api/v1/characters ===");

    let body_str = std::str::from_utf8(&body).unwrap_or("");
    tracing::info!("  Body: {}", body_str);

    // Tentar parse do body, mas aceitar valores parciais
    let req: CreateCharacterRequest = match serde_json::from_slice(&body) {
        Ok(r) => r,
        Err(e) => {
            tracing::error!("  Failed to parse create character request: {}", e);
            let response = serde_json::json!({"success": false, "error": "Invalid request body"});
            return axum::response::Response::builder()
                .status(400)
                .header("Content-Type", "application/json")
                .body(axum::body::Body::from(response.to_string()))
                .unwrap();
        }
    };

    let guid = Uuid::new_v4();
    let unique_name = req.name.to_lowercase();
    let gender_int = match req.gender.as_deref() {
        Some("female") => 1,
        _ => 0,
    };

    // Get first account as default (TODO: session-based)
    let account_id = sqlx::query_scalar::<_, Uuid>("SELECT id FROM accounts LIMIT 1")
        .fetch_optional(&state.db)
        .await
        .unwrap_or(None)
        .unwrap_or_else(Uuid::nil);

    let frame_sdb_id = req.start_class_id.unwrap_or(76331) as i32;

    let result = sqlx::query(
        r#"INSERT INTO characters
        (character_guid, account_id, name, unique_name, gender, race, head, eye_color,
         lip_color, hair_color, facial_hair_color, skin_color, voice_set, frame_sdb_id)
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14)"#,
    )
    .bind(guid)
    .bind(account_id)
    .bind(&req.name)
    .bind(&unique_name)
    .bind(gender_int)
    .bind(req.race.as_deref().unwrap_or("human"))
    .bind(req.head.unwrap_or(0))
    .bind(req.eye_color_id.unwrap_or(0))
    .bind(req.lip_color.unwrap_or(0))
    .bind(req.hair_color_id.unwrap_or(0))
    .bind(req.facial_hair_color.unwrap_or(0))
    .bind(req.skin_color_id.unwrap_or(0))
    .bind(0) // voice_set as int
    .bind(frame_sdb_id)
    .execute(&state.db)
    .await;

    let response = match result {
        Ok(_) => {
            let guid_num = uuid_to_i64(&guid);
            tracing::info!("  Character created: {} (guid={})", req.name, guid_num);
            serde_json::json!({
                "success": true,
                "character_guid": guid_num
            })
        }
        Err(e) => {
            tracing::error!("  Failed to create character: {}", e);
            serde_json::json!({
                "success": false,
                "error": format!("Failed to create character: {}", e)
            })
        }
    };

    axum::response::Response::builder()
        .status(200)
        .header("Content-Type", "application/json")
        .body(axum::body::Body::from(response.to_string()))
        .unwrap()
}

/// POST /api/v1/characters/validate_name
/// Resposta: {"valid": true, "name": "TestPlayer"}
/// Ou: {"valid": false, "reason": ["ERR_NAME_IN_USE"]}
pub async fn validate_name(
    State(state): State<Arc<AppState>>,
    body: axum::body::Bytes,
) -> axum::response::Response {
    tracing::info!("=== POST /api/v1/characters/validate_name ===");

    let body_str = std::str::from_utf8(&body).unwrap_or("");
    tracing::info!("  Body: {}", body_str);

    let req: ValidateNameRequest = match serde_json::from_slice(&body) {
        Ok(r) => r,
        Err(e) => {
            tracing::warn!("  Failed to parse validate_name body: {}", e);
            // Retornar valid=true se nao conseguir parsear (nao bloquear)
            let response = serde_json::json!({"valid": true, "name": ""});
            return axum::response::Response::builder()
                .status(200)
                .header("Content-Type", "application/json")
                .body(axum::body::Body::from(response.to_string()))
                .unwrap();
        }
    };

    let name = req.name.trim();

    if name.len() < 3 || name.len() > 20 {
        let response = serde_json::json!({
            "valid": false,
            "reason": ["ERR_NAME_LENGTH"]
        });
        return axum::response::Response::builder()
            .status(200)
            .header("Content-Type", "application/json")
            .body(axum::body::Body::from(response.to_string()))
            .unwrap();
    }

    let existing = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM characters WHERE unique_name = $1 AND deleted_at IS NULL",
    )
    .bind(name.to_lowercase())
    .fetch_one(&state.db)
    .await
    .unwrap_or(0);

    let response = if existing > 0 {
        serde_json::json!({
            "valid": false,
            "reason": ["ERR_NAME_IN_USE"]
        })
    } else {
        serde_json::json!({
            "valid": true,
            "name": name
        })
    };

    axum::response::Response::builder()
        .status(200)
        .header("Content-Type", "application/json")
        .body(axum::body::Body::from(response.to_string()))
        .unwrap()
}

/// POST /api/v1/characters/{guid}/delete
/// O client envia o guid como NUMERO i64 (convertido por uuid_to_i64), nao como UUID string.
/// Precisamos encontrar o personagem comparando o i64 com todos os UUIDs no banco.
pub async fn delete_character(
    State(state): State<Arc<AppState>>,
    Path(guid): Path<String>,
) -> axum::response::Response {
    tracing::info!("=== POST /api/v1/characters/{}/delete ===", guid);

    // Buscar todos os personagens e comparar o i64 derivado do UUID
    if let Ok(guid_num) = guid.parse::<i64>() {
        let all_chars = sqlx::query_as::<_, Character>(
            "SELECT * FROM characters WHERE deleted_at IS NULL"
        )
        .fetch_all(&state.db)
        .await
        .unwrap_or_default();

        for c in &all_chars {
            if uuid_to_i64(&c.character_guid) == guid_num {
                tracing::info!("  Found character to delete: {} (uuid={})", c.name, c.character_guid);
                let _ = sqlx::query("UPDATE characters SET deleted_at = NOW() WHERE character_guid = $1")
                    .bind(c.character_guid)
                    .execute(&state.db)
                    .await;
                break;
            }
        }
    } else {
        // Fallback: tentar como UUID direto
        if let Ok(uuid) = Uuid::parse_str(&guid) {
            let _ = sqlx::query("UPDATE characters SET deleted_at = NOW() WHERE character_guid = $1")
                .bind(uuid)
                .execute(&state.db)
                .await;
        }
    }

    let response = serde_json::json!({"success": true});
    axum::response::Response::builder()
        .status(200)
        .header("Content-Type", "application/json")
        .body(axum::body::Body::from(response.to_string()))
        .unwrap()
}

/// POST /api/v2/characters/{guid}/undelete
pub async fn undelete_character(
    State(state): State<Arc<AppState>>,
    Path(guid): Path<String>,
) -> axum::response::Response {
    tracing::info!("=== POST /api/v2/characters/{}/undelete ===", guid);

    if let Ok(guid_num) = guid.parse::<i64>() {
        let all_chars = sqlx::query_as::<_, Character>(
            "SELECT * FROM characters WHERE deleted_at IS NOT NULL"
        )
        .fetch_all(&state.db)
        .await
        .unwrap_or_default();

        for c in &all_chars {
            if uuid_to_i64(&c.character_guid) == guid_num {
                let _ = sqlx::query("UPDATE characters SET deleted_at = NULL WHERE character_guid = $1")
                    .bind(c.character_guid)
                    .execute(&state.db)
                    .await;
                break;
            }
        }
    } else if let Ok(uuid) = Uuid::parse_str(&guid) {
        let _ = sqlx::query("UPDATE characters SET deleted_at = NULL WHERE character_guid = $1")
            .bind(uuid)
            .execute(&state.db)
            .await;
    }

    let response = serde_json::json!({"success": true});
    axum::response::Response::builder()
        .status(200)
        .header("Content-Type", "application/json")
        .body(axum::body::Body::from(response.to_string()))
        .unwrap()
}

/// GET /api/v2/characters/{id}/visuals
pub async fn get_character_visuals(
    Path(id): Path<String>,
) -> axum::response::Response {
    tracing::info!("=== GET /api/v2/characters/{}/visuals ===", id);
    let response = serde_json::json!({"visuals": {}});
    axum::response::Response::builder()
        .status(200)
        .header("Content-Type", "application/json")
        .body(axum::body::Body::from(response.to_string()))
        .unwrap()
}

/// POST /api/v2/characters/{id}/visual_loadouts/0/purchase_and_update
pub async fn update_visuals(
    Path(id): Path<String>,
) -> AppResult<GenericResponse> {
    tracing::info!("=== POST /api/v2/characters/{}/visual_loadouts/0/purchase_and_update ===", id);
    Ok(Json(GenericResponse {
        success: true,
        error: None,
    }))
}

// ============================================================================
// Servers / Zones
// ============================================================================

/// GET+POST /api/v1/server/list
/// O client chama via POST com body {"build": "unique_build_id"} mas o engine tambem pode chamar via GET.
/// Resposta: {"zone_list": [{"zone_name": "...", "matrix_url": "...", ...}]}
/// protocol_version DEVE ser 309608 (0x4B968)
pub async fn server_list(
    headers: axum::http::HeaderMap,
    body: axum::body::Bytes,
) -> axum::response::Response {
    tracing::info!("=== GET/POST /api/v1/server/list ===");
    if let Ok(body_str) = std::str::from_utf8(&body) {
        if !body_str.is_empty() {
            tracing::info!("  Body: {}", body_str);
        }
    }
    for (name, value) in headers.iter() {
        tracing::info!("  Header: {}: {:?}", name, value);
    }

    let matrix_host = std::env::var("PUBLIC_MATRIX_HOST").unwrap_or_else(|_| "127.0.0.1".into());
    let matrix_port = std::env::var("MATRIX_PORT").unwrap_or_else(|_| "25000".into());

    // PIN retorna zone_list VAZIA. Isso forca pveMatchmaking=true no Lua,
    // o que faz o engine C++ usar o Oracle internamente para encontrar o servidor.
    let _ = (matrix_host, matrix_port); // unused com zone_list vazia
    let response = r#"{"zone_list":[]}"#;

    tracing::info!("  Server list response: {}", response);

    axum::response::Response::builder()
        .status(200)
        .header("Content-Type", "application/json")
        .body(axum::body::Body::from(response))
        .unwrap()
}

/// GET /api/v1/login_alerts
pub async fn login_alerts() -> axum::response::Response {
    tracing::info!("=== GET /api/v1/login_alerts ===");
    let response = serde_json::json!([]);
    axum::response::Response::builder()
        .status(200)
        .header("Content-Type", "application/json")
        .body(axum::body::Body::from(response.to_string()))
        .unwrap()
}

/// GET /api/v1/zones/queue_ids
pub async fn zone_queue_ids() -> axum::response::Response {
    tracing::info!("=== GET /api/v1/zones/queue_ids ===");
    let response = serde_json::json!([]);
    axum::response::Response::builder()
        .status(200)
        .header("Content-Type", "application/json")
        .body(axum::body::Body::from(response.to_string()))
        .unwrap()
}

/// GET /api/v2/zone_settings
pub async fn zone_settings() -> AppResult<Vec<ZoneSettings>> {
    tracing::info!("=== GET /api/v2/zone_settings ===");
    Ok(Json(vec![ZoneSettings {
        zone_id: 1,
        zone_name: "Copacabana Beta".into(),
        context: "open_world".into(),
        gametype: "pve".into(),
        max_players: 100,
    }]))
}

/// GET /api/v2/zone_settings/zone/{id}
pub async fn zone_settings_by_id(Path(id): Path<i32>) -> AppResult<ZoneSettings> {
    tracing::info!("=== GET /api/v2/zone_settings/zone/{} ===", id);
    Ok(Json(ZoneSettings {
        zone_id: id,
        zone_name: "Copacabana Beta".into(),
        context: "open_world".into(),
        gametype: "pve".into(),
        max_players: 100,
    }))
}

/// GET /api/v2/zone_settings/context/{ctx}
pub async fn zone_settings_by_context(Path(ctx): Path<String>) -> AppResult<Vec<ZoneSettings>> {
    tracing::info!("=== GET /api/v2/zone_settings/context/{} ===", ctx);
    Ok(Json(vec![ZoneSettings {
        zone_id: 1,
        zone_name: "Copacabana Beta".into(),
        context: "open_world".into(),
        gametype: "pve".into(),
        max_players: 100,
    }]))
}

/// GET /api/v2/zone_settings/gametype/{type}
pub async fn zone_settings_by_gametype(
    Path(gametype): Path<String>,
) -> AppResult<Vec<ZoneSettings>> {
    tracing::info!("=== GET /api/v2/zone_settings/gametype/{} ===", gametype);
    Ok(Json(vec![ZoneSettings {
        zone_id: 1,
        zone_name: "Copacabana Beta".into(),
        context: "open_world".into(),
        gametype: "pve".into(),
        max_players: 100,
    }]))
}

// ============================================================================
// Garage
// ============================================================================

/// GET /api/v3/characters/{id}/garage_slots
pub async fn garage_slots(Path(id): Path<String>) -> AppResult<Vec<GarageSlot>> {
    tracing::info!("=== GET /api/v3/characters/{}/garage_slots ===", id);
    Ok(Json(vec![GarageSlot {
        slot_id: 1,
        battleframe: "assault".into(),
        level: 1,
        xp: 0,
    }]))
}

/// GET /api/v3/garage_slots/battleframes_for_sale
pub async fn battleframes_for_sale() -> axum::response::Response {
    tracing::info!("=== GET /api/v3/garage_slots/battleframes_for_sale ===");
    axum::response::Response::builder()
        .status(200)
        .header("Content-Type", "application/json")
        .body(axum::body::Body::from("[]"))
        .unwrap()
}

/// POST /api/v3/characters/{id}/items/repair
pub async fn repair_items(Path(id): Path<String>) -> AppResult<GenericResponse> {
    tracing::info!("=== POST /api/v3/characters/{}/items/repair ===", id);
    Ok(Json(GenericResponse {
        success: true,
        error: None,
    }))
}

// ============================================================================
// Crafting
// ============================================================================

/// GET /api/v3/characters/{id}/manufacturing/certs
pub async fn manufacturing_certs(Path(id): Path<String>) -> axum::response::Response {
    tracing::info!("=== GET /api/v3/characters/{}/manufacturing/certs ===", id);
    axum::response::Response::builder()
        .status(200)
        .header("Content-Type", "application/json")
        .body(axum::body::Body::from("[]"))
        .unwrap()
}

/// GET /api/v3/characters/{id}/manufacturing/workbenches
pub async fn manufacturing_workbenches(Path(id): Path<String>) -> axum::response::Response {
    tracing::info!("=== GET /api/v3/characters/{}/manufacturing/workbenches ===", id);
    axum::response::Response::builder()
        .status(200)
        .header("Content-Type", "application/json")
        .body(axum::body::Body::from("[]"))
        .unwrap()
}

/// GET /api/v3/characters/{id}/manufacturing/preview
pub async fn manufacturing_preview(Path(id): Path<String>) -> axum::response::Response {
    tracing::info!("=== GET /api/v3/characters/{}/manufacturing/preview ===", id);
    axum::response::Response::builder()
        .status(200)
        .header("Content-Type", "application/json")
        .body(axum::body::Body::from("{}"))
        .unwrap()
}

// ============================================================================
// Social
// ============================================================================

/// GET /api/v1/social/friend_list
pub async fn friend_list() -> axum::response::Response {
    tracing::info!("=== GET /api/v1/social/friend_list ===");
    axum::response::Response::builder()
        .status(200)
        .header("Content-Type", "application/json")
        .body(axum::body::Body::from("[]"))
        .unwrap()
}

/// GET /api/v3/armies/{id}/members
pub async fn army_members(Path(id): Path<String>) -> axum::response::Response {
    tracing::info!("=== GET /api/v3/armies/{}/members ===", id);
    axum::response::Response::builder()
        .status(200)
        .header("Content-Type", "application/json")
        .body(axum::body::Body::from("[]"))
        .unwrap()
}

/// POST /api/v1/abuse_reports
pub async fn abuse_reports() -> AppResult<GenericResponse> {
    tracing::info!("=== POST /api/v1/abuse_reports ===");
    Ok(Json(GenericResponse {
        success: true,
        error: None,
    }))
}

// ============================================================================
// Trade
// ============================================================================

/// GET /api/v3/trade/products
pub async fn trade_products() -> axum::response::Response {
    tracing::info!("=== GET /api/v3/trade/products ===");
    axum::response::Response::builder()
        .status(200)
        .header("Content-Type", "application/json")
        .body(axum::body::Body::from("[]"))
        .unwrap()
}

// ============================================================================
// Migration
// ============================================================================

/// POST /api/v3/characters/{guid}/migrations/jan2016
pub async fn migration_jan2016(Path(guid): Path<String>) -> AppResult<GenericResponse> {
    tracing::info!("=== POST /api/v3/characters/{}/migrations/jan2016 ===", guid);
    Ok(Json(GenericResponse {
        success: true,
        error: None,
    }))
}

// ============================================================================
// Frontend & Misc
// ============================================================================

/// POST /game/accounts/create.json
pub async fn create_account_web(
    State(state): State<Arc<AppState>>,
    Json(req): Json<CreateAccountRequest>,
) -> AppResult<CreateAccountResponse> {
    tracing::info!("=== POST /game/accounts/create.json ===");
    let password_hash = hash_password(&req.password);

    let id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO accounts (id, email, password_hash, display_name) VALUES ($1, $2, $3, $4)",
    )
    .bind(id)
    .bind(&req.email)
    .bind(&password_hash)
    .bind(&req.email)
    .execute(&state.db)
    .await
    .map_err(|_| err(StatusCode::INTERNAL_SERVER_ERROR, "Failed to create account"))?;

    Ok(Json(CreateAccountResponse {
        success: true,
        account_id: Some(id),
        error: None,
    }))
}

/// POST /password_reset/create_from_game_client
pub async fn password_reset() -> AppResult<GenericResponse> {
    tracing::info!("=== POST /password_reset/create_from_game_client ===");
    Ok(Json(GenericResponse {
        success: true,
        error: None,
    }))
}

/// GET /api/v1/characters/get_character_info/{id}
pub async fn get_character_info(Path(id): Path<String>) -> axum::response::Response {
    tracing::info!("=== GET /api/v1/characters/get_character_info/{} ===", id);
    let response = serde_json::json!({
        "name": "Unknown",
        "level": 1,
        "battleframe": "assault"
    });
    axum::response::Response::builder()
        .status(200)
        .header("Content-Type", "application/json")
        .body(axum::body::Body::from(response.to_string()))
        .unwrap()
}

/// POST /api/v1/client_event(s) - Telemetria - aceitar e descartar
pub async fn client_event(
    body: axum::body::Bytes,
) -> axum::response::Response {
    if let Ok(body_str) = std::str::from_utf8(&body) {
        if !body_str.is_empty() {
            tracing::debug!("Client event: {}", body_str);
        }
    }
    axum::response::Response::builder()
        .status(200)
        .header("Content-Type", "application/json")
        .body(axum::body::Body::from("{\"success\":true}"))
        .unwrap()
}

// ============================================================================
// Operator Settings
// ============================================================================

#[derive(Deserialize)]
pub struct OperatorQuery {
    pub environment: Option<String>,
    pub build: Option<u32>,
}

/// GET /check e /operator/check
/// O client contacta este endpoint primeiro para obter URLs de todos os servicos.
pub async fn operator_check(
    Query(query): Query<OperatorQuery>,
) -> AppResult<serde_json::Value> {
    let api_host = std::env::var("PUBLIC_API_HOST")
        .unwrap_or_else(|_| "http://127.0.0.1:8080".into());
    let frontend_host = std::env::var("PUBLIC_FRONTEND_HOST")
        .unwrap_or_else(|_| "http://127.0.0.1:8080".into());

    tracing::info!(
        "=== GET /check === environment={:?}, build={:?}",
        query.environment,
        query.build
    );

    Ok(Json(serde_json::json!({
        "clientapi_host": api_host,
        "frontend_host": frontend_host,
        "ingame_host": api_host,
        "store_host": frontend_host,
        "chat_server": api_host,
        "web_accounts_host": api_host,
        "web_asset_host": api_host,
        "web_host": frontend_host,
        "market_host": frontend_host,
        "replay_host": frontend_host,
        "rhsigscan_host": api_host
    })))
}

// ============================================================================
// Catch-all handler
// ============================================================================

/// Fallback handler que aceita QUALQUER rota nao mapeada e retorna 200 com {"success": true}.
/// Isso evita 404s que podem travar o client.
/// Logga a rota, metodo, headers e body para debug.
pub async fn catch_all_handler(
    method: axum::http::Method,
    uri: axum::http::Uri,
    headers: axum::http::HeaderMap,
    body: axum::body::Bytes,
) -> axum::response::Response {
    tracing::warn!("=== CATCH-ALL: {} {} ===", method, uri);
    for (name, value) in headers.iter() {
        tracing::debug!("  Header: {}: {:?}", name, value);
    }
    if let Ok(body_str) = std::str::from_utf8(&body) {
        if !body_str.is_empty() {
            tracing::debug!("  Body: {}", body_str);
        }
    }

    axum::response::Response::builder()
        .status(200)
        .header("Content-Type", "application/json")
        .body(axum::body::Body::from("{\"success\":true}"))
        .unwrap()
}

use std::sync::Arc;

use axum::routing::{get, post};
use axum::Router;

use super::handlers;
use crate::AppState;

pub fn build_router(state: Arc<AppState>) -> Router {
    Router::new()
        // === clientapi_host endpoints ===

        // Auth & Accounts
        // POST /api/v2/accounts - Info da conta (aceita body vazio)
        .route("/api/v2/accounts", post(handlers::get_account_info))
        // POST /api/v2/accounts/login - Login nativo do engine C++ (aceita body vazio)
        .route(
            "/api/v2/accounts/login",
            post(handlers::native_login).get(handlers::native_login),
        )
        .route("/api/v2/accounts/get_cookie", get(handlers::get_cookie_totp))
        .route("/api/v2/accounts/email_totp", post(handlers::email_totp))
        .route(
            "/api/v2/accounts/link_steam_account",
            post(handlers::link_steam_account),
        )
        // GET+POST /api/v2/accounts/character_slots - Aceita ambos metodos
        .route(
            "/api/v2/accounts/character_slots",
            get(handlers::character_slots).post(handlers::character_slots),
        )
        .route(
            "/api/v2/accounts/change_language",
            post(handlers::change_language),
        )

        // Oracle Ticket - CRITICO para conexao ao Matrix
        .route("/api/v1/oracle/ticket", post(handlers::oracle_ticket))

        // Characters
        .route("/api/v2/characters/list", get(handlers::list_characters))
        .route("/api/v1/characters", post(handlers::create_character))
        .route(
            "/api/v1/characters/validate_name",
            post(handlers::validate_name),
        )
        .route(
            "/api/v1/characters/:guid/delete",
            post(handlers::delete_character),
        )
        .route(
            "/api/v2/characters/:guid/undelete",
            post(handlers::undelete_character),
        )
        .route(
            "/api/v2/characters/:id/visuals",
            get(handlers::get_character_visuals),
        )
        .route(
            "/api/v2/characters/:id/visual_loadouts/0/purchase_and_update",
            post(handlers::update_visuals),
        )
        .route(
            "/api/v1/characters/get_character_info/:id",
            get(handlers::get_character_info),
        )

        // Servers & Zones
        // GET+POST /api/v1/server/list - O Lua chama via POST, engine pode chamar via GET
        .route(
            "/api/v1/server/list",
            get(handlers::server_list).post(handlers::server_list),
        )
        .route("/api/v1/login_alerts", get(handlers::login_alerts))
        .route("/api/v1/zones/queue_ids", get(handlers::zone_queue_ids))
        .route("/api/v2/zone_settings", get(handlers::zone_settings))
        .route(
            "/api/v2/zone_settings/zone/:id",
            get(handlers::zone_settings_by_id),
        )
        .route(
            "/api/v2/zone_settings/context/:ctx",
            get(handlers::zone_settings_by_context),
        )
        .route(
            "/api/v2/zone_settings/gametype/:type",
            get(handlers::zone_settings_by_gametype),
        )

        // Garage
        .route(
            "/api/v3/characters/:id/garage_slots",
            get(handlers::garage_slots),
        )
        .route(
            "/api/v3/garage_slots/battleframes_for_sale",
            get(handlers::battleframes_for_sale),
        )
        .route(
            "/api/v3/characters/:id/items/repair",
            post(handlers::repair_items),
        )

        // Crafting
        .route(
            "/api/v3/characters/:id/manufacturing/certs",
            get(handlers::manufacturing_certs),
        )
        .route(
            "/api/v3/characters/:id/manufacturing/workbenches",
            get(handlers::manufacturing_workbenches),
        )
        .route(
            "/api/v3/characters/:id/manufacturing/preview",
            get(handlers::manufacturing_preview),
        )

        // Social
        .route("/api/v1/social/friend_list", get(handlers::friend_list))
        .route(
            "/api/v3/armies/:id/members",
            get(handlers::army_members),
        )
        .route("/api/v1/abuse_reports", post(handlers::abuse_reports))

        // Trade
        .route("/api/v3/trade/products", get(handlers::trade_products))

        // Migration
        .route(
            "/api/v3/characters/:guid/migrations/jan2016",
            post(handlers::migration_jan2016),
        )

        // === frontend_host endpoints ===
        .route(
            "/game/accounts/create.json",
            post(handlers::create_account_web),
        )

        // === web_accounts_host endpoints ===
        .route(
            "/password_reset/create_from_game_client",
            post(handlers::password_reset),
        )

        // Login customizado do emulador
        .route("/api/v1/auth/login", post(handlers::login))

        // Client events (telemetry - accept and discard)
        .route("/api/v1/client_event", post(handlers::client_event))
        .route("/api/v1/client_events", post(handlers::client_event))

        // === Operator endpoint (client contacts this first) ===
        .route("/check", get(handlers::operator_check))
        .route("/operator/check", get(handlers::operator_check))

        // State para handlers que usam DB
        .with_state(state)

        // === Catch-all fallback ===
        // Aceita QUALQUER rota nao mapeada e retorna 200 com {"success": true}
        // Deve ser o ultimo - evita 404s que podem travar o client
        .fallback(handlers::catch_all_handler)
}

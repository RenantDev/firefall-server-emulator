# Firefall Server Emulator Project

## Sobre o Projeto
Emulador de servidor para o client do Firefall (MMO Shooter da Red5 Studios, Steam AppID 227700).
O jogo foi encerrado oficialmente e este projeto visa recriar o servidor para permitir gameplay offline/privado.

## Arquitetura do Client (Descoberta via Engenharia Reversa)

### Executaveis Principais
- `system/bin/FirefallClient.exe` - Client principal do jogo (32-bit, MSVC 2013)
- `system/bin/Launcher.exe` - Launcher (.NET)
- `system/bin/FirefallProcess.exe` - Processo auxiliar
- `system/bin/RedHanded.exe` - Crash reporter
- `system/bin/DumpTruck.exe` - Dump utility
- `system/bin/HWStats.exe` - Hardware stats

### DLLs Importantes
- `awesomium.dll` - Embedded Chromium (UI web)
- `GPKitClt.dll` - GameGuard anti-cheat (precisa ser bypassado/emulado)
- `steam_api.dll` - Integraçao Steam
- `SDL.dll` - Input handling
- `libcurl.dll` - HTTP requests
- Codecs: avcodec, avutil, avformat, libmp3lame (media)
- TwitchSDK - integração Twitch (pode ser ignorada)

### UI System
- **Engine de UI**: Custom XML + Lua scripting
- **Schemas**: `system/gui/schemas/component.xsd`
- **Componentes**: `system/gui/components/` (LoginUI, MainUI, Broadcast, etc.)
- **Bibliotecas Lua**: `system/gui/lib/` (lib_Liaison, lib_WebCache, lib_ChatLib, etc.)
- **Skins**: `system/gui/skins/`
- Usa Awesomium para conteudo web in-game

### Sistema de Assets
- **Formato**: `.lpf` (arquivos pack 0-7 em `system/assetdb/`)
- **Texturas**: `.dds` (DirectDraw Surface)
- **Meshes**: `.bMesh` (formato custom)
- **Materiais**: `.mtl`, `.shd`, `.shdmod` (em `system/engine/`)
- **Shaders**: HLSL (`system/engine/include/material.hlsl`)

## Protocolo de Rede (Endpoints Descobertos)

### Operator Settings (Servidores que o client espera)
O client usa `System.GetOperatorSetting(key)` para obter URLs dos servidores:
- `clientapi_host` - API principal (REST) - **PRIORIDADE MAXIMA**
- `frontend_host` - Frontend web (criação de conta, suporte)
- `ingame_host` - Conteudo web in-game (store, webframes)
- `store_host` - Loja do jogo
- `web_accounts_host` - Gestao de contas web
- `web_asset_host` - CDN de assets

### Fluxo de Login
1. `System.RequestLogin(account, password, hardwareCookie, rememberPw)` → nativo
2. Client dispara `ON_LOGIN_RESPONSE` com sucesso/falha
3. Suporta Steam auth (auto-login) e TOTP 2FA
4. Apos login: lista de personagens + seleção de zona
5. `System.StartInstanceConnection(matrix_url)` → conecta ao game server (Matrix)
6. `System.CharacterLogin(servername, serverhost, character, autoselectzone)`

### API REST Endpoints (clientapi_host)

#### Autenticação & Contas
- `POST /api/v2/accounts` - Info da conta
- `GET /api/v2/accounts/get_cookie?totp={code}` - Auth TOTP
- `POST /api/v2/accounts/email_totp` - Enviar código TOTP por email
- `POST /api/v2/accounts/link_steam_account` - Vincular Steam
- `POST /api/v2/accounts/character_slots` - Slots de personagem
- `POST /api/v2/accounts/change_language` - Mudar idioma

#### Personagens
- `GET /api/v2/characters/list` - Listar personagens
- `POST /api/v1/characters` - Criar personagem
- `POST /api/v1/characters/validate_name` - Validar nome
- `POST /api/v1/characters/{guid}/delete` - Deletar personagem
- `POST /api/v2/characters/{guid}/undelete` - Restaurar personagem
- `GET /api/v2/characters/{id}/visuals` - Visuais do personagem
- `POST /api/v2/characters/{id}/visual_loadouts/0/purchase_and_update` - Atualizar visuais

#### Servidores & Zonas
- `GET /api/v1/server/list` - Lista de servidores/zonas
- `GET /api/v1/login_alerts` - Alertas de login
- `GET /api/v1/zones/queue_ids` - IDs de fila de zona
- `GET /api/v2/zone_settings` - Configurações de zona
- `GET /api/v2/zone_settings/zone/{id}` - Zona específica
- `GET /api/v2/zone_settings/context/{ctx}` - Por contexto
- `GET /api/v2/zone_settings/gametype/{type}` - Por tipo de jogo

#### Garage & Equipamento
- `GET /api/v3/characters/{id}/garage_slots` - Slots de garage
- `GET /api/v3/garage_slots/battleframes_for_sale` - Frames à venda
- `POST /api/v3/characters/{id}/items/repair` - Reparar itens

#### Crafting/Manufatura
- `GET /api/v3/characters/{id}/manufacturing/certs` - Certificações
- `GET /api/v3/characters/{id}/manufacturing/workbenches` - Bancadas
- `GET /api/v3/characters/{id}/manufacturing/preview` - Preview

#### Social
- `GET /api/v1/social/friend_list` - Lista de amigos
- `GET /api/v3/armies/{id}/members` - Membros da army
- `POST /api/v1/abuse_reports` - Denúncias

#### Comércio
- `GET /api/v3/trade/products` - Produtos do marketplace

#### Migração
- `POST /api/v3/characters/{guid}/migrations/jan2016` - Migração de dados

### Frontend Host Endpoints
- `/game/accounts/create.json` - Criação de conta
- `/support` - Pagina de suporte
- `/founders` - Pagina de founders

### Web Accounts Host Endpoints
- `/password_reset/create_from_game_client` - Reset de senha

## APIs Nativas do Client (Lua bindings)

### System.*
- `GetClientTime()`, `GetElapsedTime(timestamp)`
- `PlaySound(sound)`, `Shutdown()`
- `GetCvar(key)`, `SetCvar(key, value)`, `BufferCommand(cmd)`
- `GetConfig(section, key)`, `GetOperatorSetting(key)`
- `RequestLogin(account, password, cookie, remember)`
- `CharacterLogin(servername, serverhost, character, autoselectzone)`
- `StartInstanceConnection(matrix_url)`
- `GetServerList()`, `GetCharacterList()`, `GetMapList()`
- `GetBuildInfo()`, `GetLocale()`, `GetArg(key)`
- `SetRememberLogin(bool)`, `GetRememberLogin()`
- `IsDevMode()`, `GoLookingGlass()`
- `PopUrl(url)`, `SignalMovieStarted()`

### Player.*
- `GetInfo()` → name, class, faction, race, gender
- `IsReady()`, `GetTraumaLevel()`
- `GetLifeInfo()` → Health, MaxHealth, Shield, MaxShield
- `GetAbilities()`, `GetSelectedAbility()`, `GetSelectedWeapon()`
- `GetWeaponInfo(idx)`, `GetBattleFrames()`
- `GetReticleInfo()`, `GetStatusEffects()`
- `GetArmyTag()`, `GetSquadRoster()`, `GetSquadVitals()`
- `GetScoreBoard()`, `GetItemInfo(itemId)`
- `GetLPCF()` → flags de lock/control
- `JoinMission()`, `IsSquadLeader()`, `GetTargetId()`

### Game.*
- `SendChatMessage(channel, text)`, `SendWhisper(target, text)`
- `GetDialogContent(contentId)`, `SendDialogResponse(dialogId, response)`
- `GetTransmittingVoipUsers()`
- `GetMenuList(id)`, `CallMenuAction(key)`
- `GetTargetInfo(entityId)`, `GetItemInfoByType(typeId)`

### HTTP.*
- `IssueRequest(url, method, params, callback)` - faz requisição HTTP
- `IsRequestPending()` - verifica se há req. pendente

### Component.* (UI)
- `GetFrame()`, `GetWidget()`, `CreateFrame()`, `CreateWidget()`
- `PostMessage()`, `GenerateEvent()`, `GetInfo()`
- `GetSetting()`, `LookupText()` (i18n)

## Eventos do Client (dispatchados pelo engine nativo)
- `ON_LOGIN_RESPONSE` - resposta de login
- `ON_COMPONENT_LOAD` - componente carregado
- `ON_CHAT_MESSAGE` - mensagem de chat
- `ON_AUTHENTICATE_SUCCESS` / failure
- `ON_CHARACTER_LIST_CHANGED`
- `ON_SERVERLIST_CHANGED`
- `ON_SELECT_ZONE`
- `ON_WEB_MESSAGE_RECEIVED`
- `MY_CONNECT` - evento de conexão ao servidor
- `MY_SLASH_HANDLER` - comandos slash

## Stack Tecnologico Recomendado para o Servidor

### Linguagem: Rust (preferencial) ou Go
- Performance para game server
- Segurança de memória
- Async nativo (tokio)

### Componentes do Servidor
1. **Login Server** (REST API) - clientapi_host
   - Autenticação, gestão de contas, personagens
   - Framework: Actix-web ou Axum (Rust)
2. **Matrix Server** (Game Server)
   - Lógica de jogo, mundo, entidades
   - Protocolo binário proprietário (precisa RE do client)
3. **Frontend Server** - frontend_host
   - Páginas web simples
4. **Asset Server** - web_asset_host (CDN)
   - Servir assets estáticos

### Banco de Dados
- PostgreSQL para dados persistentes
- Redis para cache/sessões

## Diretorio do Projeto do Servidor
O código do servidor emulador deve ficar em `server/` na raiz deste repositório.

## Comandos Uteis

### Analise do Client
```bash
# Strings do executável
strings system/bin/FirefallClient.exe | grep -i "api\|http\|matrix\|login"

# Listar exports das DLLs
dumpbin /exports system/bin/FirefallClient.exe

# Monitorar trafego de rede
# Usar Wireshark/mitmproxy para capturar pacotes durante execução
```

## Referências da Comunidade
- Projetos de emulação existentes podem ter informações sobre o protocolo binário
- O testharness.lua em `system/gui/testharness.lua` documenta a interface Lua↔Engine
- Os scripts Lua do client são a melhor fonte de documentação dos endpoints REST

## Regras para Contribuição
- Nunca modificar arquivos do client original
- Documentar todo protocolo descoberto em `docs/`
- Testes automatizados para cada endpoint implementado
- Manter compatibilidade com o client build 1962 (última versão Steam)

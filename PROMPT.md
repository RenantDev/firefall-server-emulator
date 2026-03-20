# Prompt: Firefall Server Emulator - Implementação Completa

## Objetivo
Implementar um servidor emulador funcional para o client Firefall (build prod-1962, Steam AppID 227700) que permita:
1. Login (Steam auth - o client usa Steam automaticamente)
2. Criação de personagem
3. Seleção de zona e entrada no jogo (Matrix server UDP)

## Contexto do Projeto
- Servidor Rust/Axum em `server/` (dockerizado com PostgreSQL, Redis, Nginx)
- Client em `F:\SteamLibrary\steamapps\common\Firefall`
- Client apontando para servidor local via `firefall.ini` → `[Config] OperatorHost = http://127.0.0.1:8080`
- Referência principal: projeto **PIN** (github.com/themeldingwars/PIN) em C#/.NET
- Wiki: github.com/themeldingwars/Documentation/wiki

## GAPS CRÍTICOS IDENTIFICADOS

### GAP 1: O PIN usa client PATCHEADO
O PIN distribui um `FirefallClient.exe` patcheado que:
- Remove verificação do GameGuard (GPKitClt.dll)
- Aceita certificados SSL auto-assinados
- Possivelmente aceita HTTP plain
- **SEM O PATCH, o client original pode rejeitar respostas HTTP silenciosamente**

### GAP 2: Endpoint Oracle Ticket (FALTANDO)
O fluxo pós-login tem um endpoint crítico que NÃO está nos scripts Lua - é chamado pelo engine C++ nativamente:
```
POST /api/v1/oracle/ticket
Request: { "ver": 1962, "build": "1962" }
Response:
{
  "country": "US",
  "datacenter": "local",
  "hostname": "localhost",
  "matrix_url": "127.0.0.1:25000",
  "operator_override": { /* hosts */ },
  "session_id": "uuid",
  "ticket": "base64-encoded-ticket"
}
```
O ticket é necessário para o handshake UDP do Matrix server.

### GAP 3: Server List formato errado
O endpoint `/api/v1/server/list` deve ser **POST** (não GET) com body `{"build": "unique_build_id"}` e retornar:
```json
{
  "zone_list": [
    {
      "zone_name": "Copacabana Beta",
      "matrix_url": "127.0.0.1:25000",
      "owner": "root",
      "players": 0,
      "match": 0,
      "revision": 0,
      "protocol_version": 309608
    }
  ]
}
```

### GAP 4: Character List formato incompleto
O `GET /api/v2/characters/list` é chamado nativamente pelo engine (não pelo Lua) e precisa de campos específicos:
```json
{
  "is_dev": false,
  "rb_balance": 0,
  "name_change_cost": 100,
  "characters": [
    {
      "character_guid": 12345678901234,
      "name": "PlayerName",
      "unique_name": "playername",
      "is_dev": false,
      "is_active": true,
      "created_at": "2015-01-01T00:00:00Z",
      "title_id": 0,
      "time_played_secs": 0,
      "needs_name_change": false,
      "max_frame_level": 40,
      "frame_sdb_id": 76331,
      "current_level": 1,
      "gender": 0,
      "current_gender": "male",
      "elite_rank": 0,
      "last_seen_at": "2025-01-01T00:00:00Z",
      "visuals": {},
      "gear": [],
      "expires_in": null,
      "deleted_at": null,
      "race": "human",
      "migrations": []
    }
  ]
}
```

### GAP 5: Matrix Server UDP (FALTANDO COMPLETAMENTE)
O game server usa **UDP** (não TCP!) com protocolo customizado:

**Handshake (4 passos)**:
```
1. Client → Server: [SocketID=0x00000000][POKE][ProtocolVersion=0x0004B968]
2. Server → Client: [SocketID=0x00000000][HEHE][AssignedSocketID]
3. Client → Server: [SocketID=Assigned][KISS][ProtocolVersion][StreamingProtocol=0x4C5F]
4. Server → Client: [SocketID=Assigned][HUGG][SequenceStart][GameServerPort]
```

**Formato de pacotes pós-handshake**:
```
[uint32 SocketID] [uint16 Header]
Header bits: [15-14: Channel] [13-12: Resend] [11: Split] [10-0: Length]
```

**Canais**: 0=Unreliable, 1=Reliable abstract, 2=Reliable+GSS, 3=Unreliable+GSS

### GAP 6: Red5 Signature Auth
O client assina TODA request com:
```
X-Red5-Signature: Red5 <token> ver=2&tc=<timestamp>&nonce=<hex>&uid=<base64>&host=<host>&path=<path>&hbody=<sha1_body>&cid=0
```
Onde:
- **uid** = `base64(SHA1(email + '-red5salt-2239nknn234j290j09rjdj28fh8fnj234k'))`
- **token** = `hex(SHA1(email + '-' + password + '-red5salt-7nc9bsj4j734ughb8r8dhb8938h8by987c4f7h47b'))`

Para o emulador: **NÃO validar** - aceitar qualquer assinatura.

---

## Fluxo Completo de Login → Jogo

```
1. Client inicia
2. GET {OperatorHost}/check?environment=prod&build=1962
   → JSON com todos os hosts (clientapi_host, frontend_host, etc.)
3. Tela de login exibida
4. Player clica Login (Steam auth automático)
5. POST {clientapi_host}/api/v2/accounts/login
   Headers: X-Steam-Session-Ticket, X-Steam-Client-UserId, X-Red5-Signature
   Body: vazio (content-length: 0)
   → JSON com can_login, is_dev, character_limit, etc.
6. Engine dispara ON_LOGIN_RESPONSE → Lua processa
7. POST /api/v1/client_event (telemetria)
8. GET /api/v1/login_alerts
9. MY_CONNECT event → ZoneSelection
10. CheckEULA (local, sem HTTP)
11. System.RefreshCharactersList() (nativo)
    → GET {clientapi_host}/api/v2/characters/list
    → Engine dispara ON_LIST_CHARACTERS_RESPONSE
12. POST {clientapi_host}/api/v1/server/list com {build: "..."}
    → zone_list com matrix_url de cada zona
13. GET /api/v2/accounts/character_slots
14. Player seleciona personagem e zona, clica Enter World
15. POST /api/v2/accounts/change_language
16. POST /api/v1/oracle/ticket (NATIVO, NÃO NO LUA!)
    → Retorna matrix_url e ticket para conexão UDP
17. System.StartInstanceConnection(matrix_url) (nativo)
18. UDP POKE → HEHE → KISS → HUGG (handshake Matrix)
19. Matrix Login message com ticket
20. Server envia WelcomeToTheMatrix → EnterZone
```

---

## Headers que o Client Envia (TODA request)

```
Connection: Keep-Alive
Accept-Language: en
User-Agent: netlib2/prod-1962 (Microsoft Windows 8 build 9200, 64-bit)
X-Red5-Hotfix-Level: 0
X-Red5-Is-Censored: 0
X-Red5-Signature: Red5 {token} ver=2&tc={ts}&nonce={hex}&uid={b64}&host={host}&path={path}&hbody={sha1}&cid=0
X-Red5-Signature2: {b64},{b64},{b64},{b64}
X-Red5-SiloPoolId: 0
X-Steam-Client-UserId: 76561198047218480
X-Steam-Session-Ticket: FAAAAK9z0k2Js...
Content-Length: 0
Host: 127.0.0.1:8080
```

---

## Formato EXATO das Respostas

### POST /api/v2/accounts/login
```json
{
  "account_id": 12345,
  "can_login": true,
  "is_dev": false,
  "steam_auth_prompt": false,
  "skip_precursor": true,
  "cais_status": {
    "state": "none",
    "duration": 0,
    "expires_at": 0
  },
  "created_at": 1609459200,
  "character_limit": 4,
  "is_vip": true,
  "vip_expiration": 0,
  "events": {
    "count": 0,
    "results": []
  }
}
```

### GET /api/v1/login_alerts
```json
[
  {"message": "Welcome to Firefall Emulator!"}
]
```

### POST /api/v1/server/list
Request: `{"build": "unique_build_id"}`
```json
{
  "zone_list": [
    {
      "zone_name": "Copacabana Beta",
      "matrix_url": "127.0.0.1:25000",
      "owner": "root",
      "players": 0,
      "match": 0,
      "revision": 0,
      "protocol_version": 309608
    }
  ]
}
```
**Nota**: `protocol_version` deve ser `309608` (0x4B968) - matches o valor hardcoded no binário.

### POST /api/v1/oracle/ticket
Request: `{"ver": 1962, "build": "1962"}`
```json
{
  "country": "US",
  "datacenter": "local",
  "hostname": "localhost",
  "matrix_url": "127.0.0.1:25000",
  "operator_override": {},
  "session_id": "random-uuid",
  "ticket": "base64-encoded-55-byte-ticket"
}
```

### POST /api/v1/characters/validate_name
Request: `{"name": "TestPlayer", "lang": "en"}`
```json
{"valid": true, "name": "TestPlayer"}
```

### POST /api/v1/characters (criar)
Request:
```json
{
  "name": "TestPlayer",
  "start_class_id": 76164,
  "is_dev": false,
  "gender": "male",
  "head": 10002,
  "head_accessory_a": 10089,
  "eye_color_id": 77184,
  "hair_color_id": 77189,
  "skin_color_id": 77179,
  "voice_set": "voicePrint"
}
```

### POST /api/v1/client_event
Aceitar qualquer body, retornar `{"success": true}`

---

## Configuração Completa do firefall.ini

```ini
[Config]
OperatorHost = http://127.0.0.1:8080
UsesLauncher = false

[UI]
PlayIntroMovie = false

[Debug]
LogLevel-HTTP = debug
LogLevel-Operator = debug
LogLevel-Login = debug
```

---

## Arquitetura do Servidor (o que precisa existir)

### 1. HTTP API Server (porta 8080) - Rust/Axum ← JÁ EXISTE, precisa correções
- Operator check (`/check`)
- Login (`/api/v2/accounts/login`)
- Characters CRUD
- Server/zone list
- Oracle ticket (`/api/v1/oracle/ticket`) ← FALTANDO
- Client events
- Login alerts

### 2. Matrix Server UDP (porta 25000) ← FALTANDO COMPLETAMENTE
- Handshake POKE/HEHE/KISS/HUGG
- Reliable/unreliable channels
- Login message processing
- WelcomeToTheMatrix response
- EnterZone com world state

### 3. Game Server UDP (porta 25001) ← FALTANDO
- Entity state replication (Views/GSS)
- Player movement
- Combat
- Chat relay

---

## Como Buildar e Rodar

```bash
export PATH="/c/Program Files/Docker/Docker/resources/bin:${PATH}"

# Build
docker compose -f "F:/SteamLibrary/steamapps/common/Firefall/docker-compose.yml" build --no-cache api

# Start
docker compose -f "F:/SteamLibrary/steamapps/common/Firefall/docker-compose.yml" up -d

# Logs
docker logs firefall-api --tail 50

# Rebuild rápido (se cache funcionar)
docker compose -f "F:/SteamLibrary/steamapps/common/Firefall/docker-compose.yml" up --build -d api
```

---

## Referências

### Projetos da Comunidade
- **PIN** (servidor principal): github.com/themeldingwars/PIN
- **RIN.WebAPI** (web API): github.com/themeldingwars/RIN.WebAPI
- **AeroMessages** (definições de pacotes): github.com/themeldingwars/AeroMessages
- **FauFau** (lib de formatos/protocolos): github.com/themeldingwars/FauFau
- **Documentation Wiki**: github.com/themeldingwars/Documentation/wiki
- **PacketPeep** (debugger de rede): github.com/themeldingwars/PacketPeep

### Wiki Pages Importantes
- Authentication: wiki/Authentication
- firefall.ini: wiki/firefall.ini
- Game Server Protocol: wiki/Game-Server-Protocol-Overview
- MatrixServer POKE: wiki/MatrixServer-POKE
- GameServer Matrix Login: wiki/GameServer-Matrix-Login
- Web API: wiki/Web-API

### Arquivos do Client para Referência
- `system/gui/components/LoginUI/AccountLogin/AccountLogin.lua` - Login flow
- `system/gui/components/LoginUI/ZoneSelection/ZoneSelection.lua` - Zone/char selection
- `system/gui/components/LoginUI/CharacterCreation/CharacterCreation.lua` - Char creation
- `system/gui/components/LoginUI/LoginPreloadScreen/LoginPreloadScreen.lua` - Oracle → Matrix
- `system/gui/lib/lib_WebCache.lua` - URL shortcuts
- `system/gui/testharness.lua` - Mock API nativa

### Documentação Gerada
- `docs/protocols/binary_analysis_login_matrix.md` - Análise do binário

# Analise Binaria do FirefallClient.exe - Login, Sessao e Protocolo Matrix

Data da analise: 2026-03-19
Binario: `system/bin/FirefallClient.exe` (32-bit, MSVC 2013)
Build path: `C:\buildroot\environment\prod\gen\client\products\client\FirefallClient.pdb`
Source path: `c:\buildroot\environment\prod\svn_main\common\core\netlib\`

---

## 1. Login Nativo (System.RequestLogin)

### Endpoint
```
POST %s/api/v2/accounts/login
```
Onde `%s` e o valor de `clientapi_host` obtido do Operator.

### Fluxo completo

1. O Lua chama `System.RequestLogin(account, password, savedPassword)` (binding nativo em `0x017617B8`)
2. O engine C++ busca `clientapi_host` do operator settings (`0x0176099C`)
3. Constroi URL: `%s/api/v2/accounts/login` (`0x01760964`)
4. Verifica se ja existe login pendente: `"A login is already pending"` (`0x01760980`)
5. Envia POST com Content-Type: `application/json` (`0x017609C0`)

### Campos JSON da RESPOSTA que o engine parseia

O engine C++ parseia os seguintes campos do JSON de resposta (extraidos do offset `0x01760A68` - `0x01760B60`):

**Campos de sucesso:**
| Campo | Offset | Descricao |
|-------|--------|-----------|
| `can_login` | 0x017609AC | Boolean - se o login foi aceito |
| `is_dev` | 0x017609B8 | Boolean - conta de desenvolvedor |
| `character_limit` | 0x01760A68 | Integer - limite de personagens |
| `is_vip` | 0x01760A78 | Boolean - status VIP |
| `duration` | 0x01760A80 | Integer - duracao da sessao |
| `expires_at` | 0x01760A8C | Timestamp - quando expira |
| `cais_status` | 0x01760A98 | String - status CAIS (anti-addiction chinesa) |
| `state` | 0x01760AA4 | String - estado do login |
| `steam_auth_prompt` | 0x01760AAC | Boolean - solicitar auth Steam |
| `skip_precursor` | 0x01760AC0 | Boolean - pular tutorial |
| `vip_expiration` | 0x01760B18 | Timestamp - expiracao VIP |

**Campos de erro:**
| Campo | Offset | Descricao |
|-------|--------|-----------|
| `status` | 0x01760B34 | String - status HTTP textual |
| `code` | 0x01760B3C | Integer - codigo de erro |
| `error` | 0x01760B48 | String - mensagem de erro |

**Campos de dados de personagem (dentro da resposta):**
| Campo | Offset | Descricao |
|-------|--------|-----------|
| `character_guid` | 0x01760C68 | GUID (uint64) do personagem |
| `name` | 0x01760C78 | Nome do personagem |
| `characters` | 0x01760C80 | Array de personagens |
| `visuals` | 0x01760C8C | Dados visuais do personagem |

### Validacao de Content-Type

**SIM, o engine valida o Content-Type!**

```
0x017609D4 | Invalid content type: %s, content-length: %u
```

O engine espera `application/json` e rejeita respostas com content-type diferente.

### Validacao HTTP Status

```
0x01760A14 | HTTP status: %u
0x01760A24 | Connection failure: type=%s
0x01760A40 | %s Site Error. Please try again later.
```

O engine loga o HTTP status e trata erros de conexao.

### Erros de login

```
0x01760664 | LOGIN_ERR_UNKNOWN
0x01760BF8 | ERR_INVALID_LOGIN
0x01760BE8 | login_failed
0x01760A04 | Invalid Request
```

### JSON Parsing

O engine usa `sl::serialize` para deserializar JSON:
```
0x01760B60 | Unable to parse json for HTTP %d message: %s
0x01760C0C | Unable to parse json for HTTP %d message: %s
```

### Resposta JSON esperada (reconstruida)

```json
{
  "can_login": true,
  "is_dev": false,
  "is_vip": false,
  "vip_expiration": "2026-01-01T00:00:00Z",
  "character_limit": 4,
  "duration": 86400,
  "expires_at": "2026-01-02T00:00:00Z",
  "cais_status": "normal",
  "state": "ok",
  "steam_auth_prompt": false,
  "skip_precursor": true,
  "characters": [
    {
      "character_guid": 12345678901234,
      "name": "PlayerName",
      "visuals": {}
    }
  ]
}
```

Resposta de erro:
```json
{
  "status": "error",
  "code": 401,
  "error": "Invalid account or password"
}
```

---

## 2. Headers HTTP Requeridos

### Headers que o client ENVIA em TODA request HTTP

| Header | Offset | Descricao |
|--------|--------|-----------|
| `X-Red5-Hotfix-Level` | 0x01760274 | Nivel de hotfix do client |
| `X-Red5-Session-ID` | 0x0176028C | ID de sessao Red5 (apos login) |
| `X-Red5-Signature2` | 0x017602F4 | Assinatura criptografica da request |
| `X-Red5-SiloPoolId` | 0x01760384 | ID do silo pool |
| `X-Red5-Is-Censored` | 0x017603B4 | Flag de censura (regiao) |
| `Accept-Language` | 0x017603A4 | Idioma do client |
| `X-Red5-Signature` | 0x017B3F28 | Assinatura da request (versao 1) |
| `Content-Type` | (WinHTTP) | Content-Type da request |

### Headers de autenticacao por plataforma (mutuamente exclusivos)

| Header | Offset | Descricao |
|--------|--------|-----------|
| `X-369-Session-Ticket` | 0x01760308 | Ticket da plataforma 369 (China) |
| `X-369-Session-Site` | 0x01760320 | Site da plataforma 369 |
| `X-Steam-Session-Ticket` | 0x01760334 | Ticket de sessao Steam |
| `X-Steam-Client-UserId` | 0x01760364 | UserID do client Steam (formato: %llu) |
| `X-Youxi-Session-Ticket` | 0x0176034C | Ticket Youxi (China) |

### O server DEVE retornar

O engine espera `Content-Type: application/json` na resposta. O header `content-type` e parseado em lowercase (`0x017B3FEC`).

### Session ID

Apos login bem-sucedido, o engine armazena um `session_id` (`0x01750F74`) que e enviado como `X-Red5-Session-ID` em requests subsequentes.

---

## 3. Session Management

### Como funciona

1. Login retorna um `session_id` no JSON
2. O engine armazena internamente e envia como header `X-Red5-Session-ID`
3. O engine tambem usa `GetSessionTicketBase64` (`0x01747E14`) para codificar tickets
4. Steam usa `X-Steam-Session-Ticket` para autenticacao automatica
5. Hardware cookie e salvo em disco com encriptacao: `%s-Pa55fi1E_$4Lt-%s` (`0x01760840`)
6. `LoginRemember` e `LoginName` sao cvars persistentes para re-login

### Strings de sessao relevantes
```
0x01750F74 | session_id
0x0176028C | X-Red5-Session-ID
0x017618CC | LoginRemember
0x017618DC | LoginName
0x01760860 | Unable to decrypt hardware cookie at %s
0x01760DBC | autologin-savedPassword
```

### Oracle Ticket

O client usa um "Oracle" para obter tickets de conexao ao Matrix:
```
POST %s/api/v1/oracle/ticket
```

Campos da resposta do Oracle:
| Campo | Offset |
|-------|--------|
| `matrix_url` | 0x01750E44 |
| `ticket` | 0x01750E50 |
| `datacenter` | 0x01750E58 |
| `status` | 0x01750EAC |
| `message` | 0x01750EB4 |
| `retry_response` | 0x01750E2C |
| `session_id` | 0x01750F74 |
| `hostname` | 0x01750F80 |

---

## 4. Character List Nativo

### Endpoint
```
GET %s/api/v2/characters/list
```
Onde `%s` = `clientapi_host` (`0x01761274`)

### Fluxo

1. `System.RefreshCharactersList()` (`0x01761DC0`) faz a request HTTP
2. O engine espera `Content-Type: application/json` (`0x017611C8`)
3. Envia header `build` com numero do build (`0x017611DC`)
4. Parseia a resposta como JSON com campos: `character_guid`, `name`, `visuals`
5. Armazena `LastCharacterID` para auto-login (`0x017612A0`)

### Dados de personagem parseados
```
0x01761370 | gender       (string: "male"/"female")
0x01761378 | race         (string: "human")
0x01760C68 | character_guid (uint64)
0x01760C78 | name         (string)
0x01760C8C | visuals      (object)
```

### Endpoint de dados do personagem
```
GET %s/api/v1/characters/%llu/data
GET %s/api/v1/characters/%llu/data?namespace=%s&key=%s
```

### Server List
```
GET %s/api/v1/server/list
```

### Erros
```
0x01760CAC | characterlist_failed
0x01760D04 | OnRefreshCharactersResponse %u: %s
0x01760D6C | OnRefreshCharactersResponse: unpack_exception %s
```

---

## 5. Matrix Connection (Game Server)

### Protocolo: UDP com GameSocket customizado

O Matrix usa **UDP** com uma camada de confiabilidade customizada implementada em `nlgamesocket.cpp` e `nlchannels.cpp`.

Source path: `c:\buildroot\environment\prod\svn_main\common\core\netlib\nlgamesocket.cpp`

### Porta padrao
```
0x01862C08 | :25000
```
A porta padrao do Matrix e **25000**.

### Handshake UDP (4-way)

O GameSocket usa um handshake de 4 pacotes com magic bytes de 4 caracteres:

| Passo | Magic | Direcao | Offset | Descricao |
|-------|-------|---------|--------|-----------|
| 1 | `INIT` | Client -> Server | 0x017B5454 | Cliente inicia conexao |
| 2 | `HEHE` | Server -> Client | 0x017B5BCC | Servidor responde |
| 3 | `HUGG` | Client -> Server | 0x017B5BD4 | Cliente confirma |
| 4 | `CONNECT` | Client -> Server | 0x017B54C4 | Conexao estabelecida |

GameServer usa:
| Magic | Offset | Descricao |
|-------|--------|-----------|
| `POKE` | 0x017B7DDC | Server poke |
| `KISS` | 0x017B7DE4 | Server keepalive |
| `ABRT` | 0x017B5CC4 | Abort connection |

Strings de handshake:
```
0x017B5454 | %s Sent INIT (timeout=%u, count=%d)
0x017B54C4 | %s Sent CONNECT (timeout=%u, count=%d)
0x017B542C | %s Sent TRANSFER (timeout=%u, count=%d)
0x01862C9C | matrix_connectionTest: %s connected, waiting for handshake
0x01862D0C | matrix_connectionTest: %s handshake timed out
```

### Canais de comunicacao

O GameSocket suporta multiplos canais (channels) com diferentes garantias:
- Canal **unreliable**: sem garantia de entrega (`0x01862B0C`)
- Canal **reliable**: com garantia de entrega, ordering, e retransmissao (`0x01862B7C`)
- Cada canal tem controle de sequencia, ACK, e janela de envio

### Estados do GameSocket
```
NotOpened -> Initializing -> Connecting -> Connected -> Transferring -> Closed -> Destroyed
```

### Erros de conexao
```
SOCKERR_INVALID
SOCKERR_UNKNOWN
SOCKERR_SYSTEM_SOCKET_ERROR
SOCKERR_PROTOCOL_MISMATCH
SOCKERR_CLOSED_BY_REMOTE_REQUEST
SOCKERR_CLOSED_BY_LOCAL_REQUEST
SOCKERR_IDLE_TIMEOUT
SOCKERR_PROTOCOL_ABORT
SOCKERR_CONNECT_TIMEOUT
SOCKERR_CONNECT_REFUSED
SOCKERR_EXCEEDED_MAX_RESENDS
SOCKERR_FILL_TIMEOUT
SOCKERR_EXT_BUFFER_FULL
SOCKERR_UNPACK_FAILURE
SOCKERR_SERVER_FULL
```

### Protocol Version

```
0x01747C64 | GetProtocolVersion
0x01866068 | protocol_version
0x01760214 | login.allowBadZoneProtocol
0x017B7BD0 | Protocol Version Mismatch
0x017B60DA | Protocol mismatch in replay stream: %u != %u
```

O engine tem uma funcao `GetProtocolVersion` que retorna o numero da versao do protocolo. Se houver mismatch, a conexao e recusada com `Protocol Version Mismatch`.

### Mensagens do Protocolo Matrix (matrix_fury)

Estas sao TODAS as mensagens do protocolo Matrix, extraidas dos RTTI do binario:

**Mensagens Server -> Client (recebidas):**
```
WelcomeToTheMatrix     - Primeira mensagem apos handshake
EnterZone              - Entrar em zona
ExitZone               - Sair de zona
MatrixStatus           - Status do Matrix
Announce               - Anuncio broadcast
GamePaused             - Jogo pausado
ForceUnqueue           - Forcar saida de fila
HotfixLevelChanged     - Mudanca de hotfix
UpdateZoneTimeSync     - Sincronizacao de tempo
ZoneQueueUpdate        - Update de fila de zona
MatchQueueResponse     - Resposta de fila de match
MatchQueueUpdate       - Update de fila de match
FoundMatchUpdate       - Match encontrado
UpdateDevZoneInfo      - Info de zona dev
ReceiveEmergencyChat   - Chat de emergencia
LFGLeaderChange        - Mudanca de lider LFG
LFGMatchFound          - Match LFG encontrado
SigscanData            - Dados de anti-cheat
SuperPong              - Resposta de ping
SynchronizationRequest - Pedido de sincronizacao
DebugLagSampleClient   - Amostra de lag (debug)
DebugLagSampleSim      - Amostra de lag simulacao
ServerProfiler_SendFrame - Frame de profiling
ServerProfiler_SendNames - Nomes de profiling
ChallengeInvitation    - Convite de challenge
ChallengeInvitationCancel
ChallengeInvitationResponse
ChallengeInvitationSquadInfoAck
ChallengeJoinResponse
ChallengeKicked
ChallengeLeave
ChallengeMatchParametersUpdate
ChallengeMatchStarting
ChallengeReadyCheck
ChallengeRosterUpdate
```

**Mensagens Client -> Server (enviadas):**
```
Login                  - Login no Matrix
EnterZoneAck           - ACK de entrada em zona
ExitZoneAck            - ACK de saida de zona
ClientStatus           - Status do client
ClientPreferences      - Preferencias do client
SuperPing              - Ping ao server
SynchronizationResponse - Resposta de sincronizacao
RequestPause           - Pedir pausa
RequestResume          - Pedir resume
KeyframeRequest        - Pedir keyframe
LogInstrumentation     - Log de instrumentacao
SendEmergencyChat      - Chat de emergencia
RequestSigscan         - Pedido anti-cheat
StressTestMasterObject - Teste de stress
DEV_ExecuteCommand     - Comando dev
Referee_ExecuteCommand - Comando de arbitro
ServerProfiler_RequestNames - Pedir nomes profiling
```

### Formato de mensagem

O engine usa dois tipos de envio:
```
0x01860878 | MATRIX - Sent %s          (mensagem direta)
0x018608B4 | ROUTED - Sent %s          (mensagem roteada via GSS)
0x01862D98 | GSS - Sent %s             (mensagem para Game State Server)
```

Mensagens sao serializadas usando `sl::serialize` e podem ser:
- **Direct messages**: enviadas diretamente ao Matrix
- **Routed messages**: roteadas para um GSS especifico (`RoutedMessage`, `RoutedMultipleMessage`)

### RoutedMultipleMessage
```
0x017B4374 | RoutedMultipleMessage
0x017B43A4 | %u bytes is too large for RoutedMultipleMessage
0x017B4460 | sl::serialize::pack_exception during RoutedMultipleMessage::appendMessage
```

### Views (Entity Replication)

O engine usa um sistema de "views" para replicar estado de entidades:
```
0x0175C0F4 | invalid message id %u for this view.
0x0175C0A0 | invalid shadow field index %d for this view
0x0178804C | gamesocket.viewNoMessageTimeout
0x0178806C | This should have been handled by tfMatrixConnection
```

### Security Flags
```
0x0175105C | Matrix is using security flags %u
```

---

## 6. SSL/HTTPS

### O engine usa WinHTTP (NAO libcurl para HTTP principal)

O HTTP nativo usa **WinHTTP** (Windows HTTP Services), nao libcurl:

```
0x017B41A8 | WinHttpAddRequestHeaders
0x017B4038 | WinHttpReceiveResponse
0x017B4218 | WinHttpReadData
0x017B3F50 | WinHttpWriteData
0x017B4250 | WinHttpCrackUrl
0x017B4018 | WinHttpQueryHeaders(statusCode)
```

### SSL/HTTPS suportado

```
0x017B4290 | Unable to set WINHTTP_OPTION_SECURE_PROTOCOLS
0x017B42C0 | Unable to set WinHTTP option WINHTTP_ENABLE_SSL_REVERT_IMPERSONATION
```

O engine configura `WINHTTP_OPTION_SECURE_PROTOCOLS` e `WINHTTP_ENABLE_SSL_REVERT_IMPERSONATION`.

### HTTPS obrigatorio para Oracle

```
0x01750D20 | Oracle URL [%s] not configured for HTTPS (request must be secure)
```

O endpoint `/api/v1/oracle/ticket` **EXIGE HTTPS**. O engine valida que a URL comeca com `https://`.

### HTTPS NAO e obrigatorio para login/clientapi

O formato da URL e `%s/api/v2/accounts/login` onde `%s` vem do operator. Se o operator retornar `http://`, o engine usa HTTP plain. Nao ha validacao de HTTPS para o clientapi_host.

### Certificados SSL (Thawte)

O binario contem certificados root da Thawte/Symantec embutidos:
```
0x02654924 | Thawte Certification1
0x02654B42 | http://ocsp.thawte.com0
0x026558DB | https://www.thawte.com/cps0/
```

### Erros de SSL que o engine trata
```
WINHTTP_CALLBACK_STATUS_FLAG_SECURITY_CHANNEL_ERROR
WINHTTP_CALLBACK_STATUS_FLAG_CERT_WRONG_USAGE
WINHTTP_CALLBACK_STATUS_FLAG_INVALID_CERT
WINHTTP_CALLBACK_STATUS_FLAG_CERT_REV_FAILED
WINHTTP_CALLBACK_STATUS_FLAG_CERT_DATE_INVALID
WINHTTP_CALLBACK_STATUS_FLAG_CERT_CN_INVALID
WINHTTP_CALLBACK_STATUS_FLAG_INVALID_CA
WINHTTP_CALLBACK_STATUS_FLAG_CERT_REVOKED
```

### URLs hardcoded (HTTP plain)

```
0x0173F3C8 | http://dl.firefall.com/vtex/%ENVMNEMONIC%-%BUILDNUM%/static.vtex
0x0173F610 | http://dl.firefall.com/AssetStream/%ENVMNEMONIC%-%BUILDNUM%/
0x01744224 | http://dl.firefall.com/hotfix
```

### Conclusao SSL

- **clientapi_host**: Aceita tanto HTTP quanto HTTPS (depende do operator)
- **oracle/ticket**: EXIGE HTTPS
- **dl.firefall.com**: Usa HTTP plain (assets, hotfixes)
- Para o emulador: **usar HTTP para clientapi_host e suficiente**

---

## 7. Operator System

### O que e

O Operator e o primeiro servidor que o client contacta. Ele retorna as configuracoes de todos os outros servidores.

### Host padrao
```
0x0173F0E8 | operator.firefall.com
```

### Configuracao

O client le o operator host de:
1. Argumento de linha de comando: `operator`
2. Secao `[Config]` do `firefall.ini`: chave `OperatorHost`
3. Padrao: `operator.firefall.com`

### Request do Operator

```
GET %s/check?environment=%s&build=%u
```

Onde:
- `%s` = operator host URL
- `environment` = ambiente (ex: "prod")
- `build` = numero do build

### Resposta do Operator

Retorna um JSON com pares chave-valor. As chaves conhecidas incluem:
- `clientapi_host` - Host da API REST
- `ingame_host` - Host web in-game
- `frontend_host` - Host do frontend web
- `store_host` - Loja
- `web_asset_host` - CDN de assets
- `web_accounts_host` - Gestao de contas
- `hotfix_level` - Nivel de hotfix

### operator_override (cvar)

```
0x01750F60 | operator_override
```

`operator_override` e uma cvar que permite substituir as configuracoes do operator com valores locais. Usada para desenvolvimento para apontar para servidores de teste.

### SiloPoolId

```
0x0173F7C8 | SiloPoolId
0x0173F800 | silo-pool-id
0x01760384 | X-Red5-SiloPoolId
```

O SiloPoolId identifica o cluster/silo de servidores. E obtido da configuracao e enviado como header em todas as requests.

### Eventos relacionados
```
0x017498E0 | ON_OPERATOR_DIALED_IN
```

Disparado quando o client recebe as configuracoes do operator com sucesso.

### Operador com cache

```
0x0173F8B0 | Unable to send operator HTTP request to '%s', using cached values
0x0173F8F4 | Clearing operator settings [%i]
```

O engine cacheia os valores do operator no registro do Windows e usa-os como fallback se o operator nao estiver acessivel.

### operator_list e operator_set (comandos de console)

```
0x0176B44C | operator_list     - Prints out all current operator settings
0x0176B5E8 | operator_set      - Loads new operator settings from the specified url
```

---

## 8. Error Handling

### Erros de login
| Codigo | String | Descricao |
|--------|--------|-----------|
| LOGIN_ERR_UNKNOWN | Login generico falhou |
| ERR_INVALID_LOGIN | Credenciais invalidas |
| AUTOLOGIN_ERROR_STEAM | Auto-login Steam falhou |
| AUTOLOGIN_ERROR_369 | Auto-login 369 falhou |
| AUTOLOGIN_ERROR_YOUXI | Auto-login Youxi falhou |

### Erros de conexao Matrix
| Erro | Descricao |
|------|-----------|
| matrix_connect_failed | Conexao ao Matrix falhou |
| Protocol Version Mismatch | Versao do protocolo incompativel |
| Connection Refused Host Is Full | Servidor cheio |
| Host Shutdown | Servidor desligado |

### Erros de Oracle
| Erro | Descricao |
|------|-----------|
| oracle_failed | Oracle nao respondeu |
| invalid_response | Resposta invalida do oracle |
| invalid oracle JSON | JSON do oracle mal formatado |
| Decode: invalid oracle login ticket | Ticket criptografado invalido |

### Erros de HTTP (generico)
```
Connection failure: type=%s
%s Site Error. Please try again later.
Invalid content type: %s, content-length: %u
Unable to parse json for HTTP %d message: %s
```

### Erros de Matrix
| Erro | Descricao |
|------|-----------|
| AuthFailure | Autenticacao falhou |
| GssError | Erro do Game State Server |
| ArchitectError | Erro do Architect |
| RedhandedError | Erro do sistema anti-cheat |
| InternalError | Erro interno |
| NoError | Sem erro |

### Mensagens de pacote invalido
```
Received packet with invalid message-id %u
Error while unpacking message with id %u
```

---

## 9. Implicacoes para o Emulador

### Servidor Operator (Prioridade 1)

Implementar endpoint:
```
GET /check?environment={env}&build={build}
```

Retornar JSON com configuracoes. O client pode ser configurado para usar `operator_override` ou editando o `firefall.ini` com `OperatorHost=http://localhost:8080`.

### Login Server (Prioridade 2)

Implementar:
```
POST /api/v2/accounts/login
Content-Type: application/json

Response: {
  "can_login": true,
  "is_dev": true,
  "character_limit": 4,
  "is_vip": true,
  "vip_expiration": "2099-12-31T23:59:59Z",
  "duration": 999999,
  "expires_at": "2099-12-31T23:59:59Z",
  "cais_status": "healthy",
  "state": "active",
  "steam_auth_prompt": false,
  "skip_precursor": true
}
```

Headers que o server DEVE:
- Retornar `Content-Type: application/json` (o engine valida!)
- Aceitar e ignorar os headers X-Red5-* do client

### Character List (Prioridade 3)

```
GET /api/v2/characters/list
Content-Type: application/json

Response: {
  "characters": [
    {
      "character_guid": 1234567890,
      "name": "TestPlayer",
      "visuals": {}
    }
  ]
}
```

### Oracle (Prioridade 4)

```
POST /api/v1/oracle/ticket
Content-Type: application/json

Response: {
  "status": "ok",
  "matrix_url": "localhost:25000",
  "ticket": "base64encodedticket",
  "datacenter": "local",
  "session_id": "unique-session-id",
  "hostname": "localhost"
}
```

NOTA: Este endpoint EXIGE HTTPS no client original. Sera necessario:
- Configurar HTTPS no emulador com certificado auto-assinado, OU
- Patchear a verificacao no binario

### Matrix Server (Prioridade 5)

- Protocolo: **UDP** na porta **25000**
- Handshake: INIT -> HEHE -> HUGG -> CONNECT
- Primeira mensagem apos conexao: `WelcomeToTheMatrix`
- O client entao envia: `Login` (matrix_fury)
- Servidor responde com: `EnterZone`
- Mensagens sao serializadas com `sl::serialize`
- Sistema de canais: unreliable (channel 0?) e reliable
- Views para replicacao de estado de entidades

### HTTPS - Nao e obrigatorio para a maioria dos endpoints

Somente o Oracle exige HTTPS. O `clientapi_host` aceita HTTP plain.
Para o emulador, configurar o operator para retornar URLs `http://` e suficiente para login e character list.

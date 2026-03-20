# Client Patching - Firefall Server Emulator

## Visao Geral

Para o client Firefall funcionar com um servidor emulador local, existem tres obstaculos principais:

1. **Operator Host**: O client precisa saber onde esta o servidor (em vez de `operator.firefall.com`)
2. **GameGuard (GPKitClt.dll)**: Anti-cheat que tenta contactar servidores que nao existem mais
3. **HTTPS vs HTTP**: O client original usa HTTPS para algumas conexoes

Este documento cobre todas as abordagens de patching investigadas.

---

## 1. Client Patcheado do PIN (RECOMENDADO)

O projeto **PIN (Pirate Intelligence Network)** do The Melding Wars distribui um `FirefallClient.exe` patcheado que resolve os problemas de GameGuard e HTTPS.

### Download

- **Pagina de releases**: https://github.com/themeldingwars/PIN/releases
- **Versao mais recente**: v1.2.0 "Patching in the Basics" (2023-06-02)
- **Download direto do executavel**: https://github.com/themeldingwars/PIN/releases/download/1.2.0/FirefallClient.exe
- **Tamanho**: 40,197,848 bytes (~38.3 MB)

### Instrucoes de Instalacao

1. Instalar Firefall via Steam (`steam://install/227700`)
2. Fazer backup do `FirefallClient.exe` original em `Firefall\system\bin\`
3. Substituir por o `FirefallClient.exe` patcheado do PIN
4. Editar `firefall.ini` na raiz do Firefall (ver secao 4 abaixo)
5. Instalar [.NET 6 Runtime](https://dotnet.microsoft.com/download/dotnet/6.0) (necessario pelo PIN)
6. (Opcional) Confiar em certificados auto-assinados: `dotnet dev-certs https --trust`

### O que o patch do PIN faz

- Bypassa o GameGuard (`GPKitClt.dll`) que nao funciona mais
- Permite conexao HTTP (sem exigir HTTPS)
- Mantem compatibilidade com o client build 1962

### Status atual

O `FirefallClient.exe` no nosso diretorio (`system/bin/`) **ja e o patcheado do PIN** (confirmado pelo tamanho identico de 40,197,848 bytes). Nenhuma acao adicional e necessaria para o executavel.

---

## 2. Configuracao do firefall.ini (Operator Settings)

O client le configuracoes de `firefall.ini` na raiz da instalacao do Firefall.

### Como funciona internamente

Strings encontradas no binario:
```
OperatorHost                    -> Host do servidor operator
operator.firefall.com           -> Host padrao (hardcoded)
Contacting operator at %s       -> Log de conexao
https://%s/check?environment=%s&build=%u  -> URL de check (HTTPS)
http://%s%s%s                   -> URL alternativa (HTTP)
URI missing operator host       -> Erro se nao configurado
No initial operator host was specified -> Erro
```

O client contacta o OperatorHost em `https://<host>/check?environment=<env>&build=<build>` para obter as operator settings (JSON com URLs dos servicos). O PIN patcheado permite HTTP.

### Configuracao para nosso servidor

```ini
; firefall.ini - raiz do Firefall
[Config]
OperatorHost = "localhost:4400"

[FilePaths]
AssetStreamPath = "http://localhost:4401/AssetStream/%ENVMNEMONIC%-%BUILDNUM%/"
VTRemotePath = "http://localhost:4401/vtex/%ENVMNEMONIC%-%BUILDNUM%/static.vtex"

[UI]
PlayIntroMovie = false
```

### Configuracao simplificada (nosso emulador)

Para nosso servidor HTTP em porta 8080:
```ini
[Config]
OperatorHost = http://127.0.0.1:8080
```

### Chaves importantes retornadas pelo Operator

O endpoint `/check` do operator retorna um JSON com estas chaves que o client usa via `System.GetOperatorSetting(key)`:

| Chave | Descricao |
|-------|-----------|
| `clientapi_host` | API REST principal (login, personagens, etc.) |
| `frontend_host` | Frontend web (criacao de conta) |
| `ingame_host` | Conteudo web in-game |
| `store_host` | Loja do jogo |
| `web_accounts_host` | Gestao de contas |
| `web_asset_host` | CDN de assets |

### Secoes do ini reconhecidas pelo client

| Secao | Chave | Descricao |
|-------|-------|-----------|
| `[Config]` | `OperatorHost` | URL do servidor operator |
| `[FilePaths]` | `AssetStreamPath` | CDN de assets |
| `[FilePaths]` | `VTRemotePath` | Texturas virtuais remotas |
| `[FilePaths]` | `WritePath` | Diretorio de escrita |
| `[UI]` | `PlayIntroMovie` | Desabilitar intro |

---

## 3. Stub do GPKitClt.dll (Alternativa ao PIN)

Se por algum motivo nao for possivel usar o client patcheado do PIN, pode-se criar uma DLL stub que substitui o `GPKitClt.dll`.

### Funcoes exportadas pelo GPKitClt.dll original

Analise via `strings` revelou 3 funcoes exportadas (stdcall):

```
_StartUpdate@4          -> Inicia atualizacao do GameGuard (1 parametro, 4 bytes)
_GetUpdateFailReason@0  -> Retorna razao de falha (0 parametros)
_GetNewUpdateModuleFileName@8 -> Nome do modulo atualizado (2 parametros, 8 bytes)
```

### Info do binario original

- **Tamanho**: 940,344 bytes (~918 KB)
- **Caminho de build**: `e:\gpk_project\new_gpk\gpk3.0_code\client\`
- **Contem**: libtomcrypt (SHA224, AES, CTR mode)
- **Protocolo**: Comunicacao criptografada com servidores nProtect (extintos)

### Implementacao do Stub em C

```c
// GPKitClt_stub.c
// Compilar com: cl /LD /DEF:GPKitClt.def GPKitClt_stub.c /Fe:GPKitClt.dll
// Ou com MinGW: gcc -shared -o GPKitClt.dll GPKitClt_stub.c GPKitClt.def

#include <windows.h>

// _StartUpdate@4 - Inicia GameGuard update
// Retorna 0 = sucesso
__declspec(dllexport) int __stdcall StartUpdate(int param1) {
    return 0; // Sucesso - nao faz nada
}

// _GetUpdateFailReason@0 - Retorna razao de falha
// Retorna 0 = sem falha
__declspec(dllexport) int __stdcall GetUpdateFailReason(void) {
    return 0; // Sem falha
}

// _GetNewUpdateModuleFileName@8 - Nome do modulo atualizado
// Retorna 0 = sem atualizacao
__declspec(dllexport) int __stdcall GetNewUpdateModuleFileName(int param1, int param2) {
    return 0; // Sem modulo novo
}

BOOL WINAPI DllMain(HINSTANCE hinstDLL, DWORD fdwReason, LPVOID lpvReserved) {
    return TRUE;
}
```

### Arquivo de definicao de exports (GPKitClt.def)

```def
LIBRARY GPKitClt
EXPORTS
    StartUpdate=StartUpdate @1
    GetUpdateFailReason=GetUpdateFailReason @2
    GetNewUpdateModuleFileName=GetNewUpdateModuleFileName @3
```

**NOTA**: Os nomes decorados no binario sao `_StartUpdate@4`, `_GetUpdateFailReason@0` e `_GetNewUpdateModuleFileName@8`. O compilador MSVC com `__stdcall` adiciona automaticamente essa decoracao. Se usar MinGW, pode ser necessario ajustar o `.def` file para usar os nomes decorados explicitamente.

---

## 4. HTTPS com Certificado Auto-Assinado (Alternativa)

Se usar o client **original** (nao patcheado), ele exige HTTPS para a conexao ao operator e ao clientapi.

### Evidencia no binario

```
Oracle URL [%s] not configured for HTTPS (request must be secure)
URL was '%s' but HTTPS was required by script
Contacting port 443 with HTTP (not HTTPS). Is this intentional?
https://%s:%d%s%s     -> Formato URL com porta
https://%s%s%s        -> Formato URL sem porta
```

O client usa **WinHTTP** (nao libcurl) para as conexoes ao operator/clientapi:
```
WinHttpSetOption
WinHttpSetStatusCallback
WinHttpSendRequest
WINHTTP_ENABLE_SSL_REVERT_IMPERSONATION
```

E valida certificados via Windows Certificate Store:
```
CertFreeCertificateContext
CertFindCertificateInStore
```

O binario contem certificados root da **Thawte** (CA usada originalmente):
```
https://www.thawte.com/cps0/
https://www.thawte.com/repository0
```

### Geracao de certificado auto-assinado

#### Usando OpenSSL
```bash
# Gerar chave privada e certificado
openssl req -x509 -newkey rsa:2048 -keyout server.key -out server.crt \
  -days 365 -nodes \
  -subj "/CN=localhost" \
  -addext "subjectAltName=DNS:localhost,IP:127.0.0.1"

# Converter para PFX (para uso com Rust/actix-web)
openssl pkcs12 -export -out server.pfx -inkey server.key -in server.crt
```

#### Usando dotnet dev-certs (mais simples)
```bash
# Gera e confia no certificado automaticamente
dotnet dev-certs https --trust
```

### Fazer o client confiar no certificado

Como o client usa WinHTTP + Windows Certificate Store, o certificado precisa estar no **Trusted Root Certification Authorities** do Windows:

```powershell
# PowerShell (como administrador)
Import-Certificate -FilePath server.crt -CertStoreLocation Cert:\LocalMachine\Root
```

Ou via MMC:
1. Executar `mmc.exe`
2. Arquivo -> Adicionar/Remover Snap-in -> Certificados -> Computador Local
3. Trusted Root Certification Authorities -> Certificados
4. Importar `server.crt`

### Configuracao do servidor Rust/Axum para HTTPS

```rust
// No Cargo.toml, adicionar:
// axum-server = { version = "0.7", features = ["tls-rustls"] }
// rustls = "0.23"
// rustls-pemfile = "2"

use axum_server::tls_rustls::RustlsConfig;

let config = RustlsConfig::from_pem_file("server.crt", "server.key")
    .await
    .expect("Erro ao carregar certificado TLS");

axum_server::bind_rustls("0.0.0.0:443".parse().unwrap(), config)
    .serve(app.into_make_service())
    .await
    .unwrap();
```

### Abordagem com DNS/Hosts (para client original)

Se usar o client original com HTTPS, redirecionar os dominios no `/etc/hosts` (ou `C:\Windows\System32\drivers\etc\hosts`):

```
127.0.0.1  operator.firefall.com
127.0.0.1  clientapi.firefall.com
127.0.0.1  chat.firefall.com
127.0.0.1  ingame.firefall.com
```

Nesse caso, o servidor HTTPS precisa responder com um certificado valido para esses dominios (adicionando-os ao SAN do certificado auto-assinado).

---

## 5. Resumo de Abordagens

| Abordagem | Dificuldade | Modifica Client? | Requer HTTPS? |
|-----------|-------------|-------------------|---------------|
| PIN patcheado + firefall.ini | Facil | Sim (substitui exe) | Nao |
| Stub GPKitClt.dll + HTTPS | Media | Sim (substitui dll) | Sim |
| DNS/Hosts + HTTPS + certificado | Dificil | Nao | Sim |

### Recomendacao

**Usar o client patcheado do PIN** (que ja temos instalado). Com isso:
- GameGuard e bypassado automaticamente
- HTTP funciona (sem necessidade de HTTPS/certificados)
- Basta configurar `firefall.ini` com o `OperatorHost` correto

O nosso `firefall.ini` atual esta configurado com:
```ini
[Config]
OperatorHost = http://127.0.0.1:8080
```

Isso deve funcionar diretamente com nosso servidor HTTP em Rust/Axum.

---

## 6. Detalhes Tecnicos Adicionais

### Protocolo do Operator

O client faz uma requisicao ao operator host:
```
GET /check?environment=<env>&build=<build_number>
```

A resposta esperada e um JSON com as operator settings (URLs de todos os servicos).

### Sequencia de inicializacao do client

1. Le `firefall.ini` para obter `OperatorHost`
2. Se nao encontrar, usa `operator.firefall.com` (hardcoded)
3. Contacta `https://<OperatorHost>/check?environment=<env>&build=<build>` (ou HTTP com PIN)
4. Recebe JSON com operator settings
5. Dispara evento `ON_OPERATOR_DIALED_IN` para o Lua
6. Scripts Lua usam `System.GetOperatorSetting("clientapi_host")` para obter URLs
7. Login via `clientapi_host` API REST
8. Conexao ao game server via Matrix (UDP)

### Cvar de debug relacionado
```
operator_list    -> Imprime todas as operator settings atuais
operator_set     -> Carrega novas settings de uma URL
                    Usage: operator_set <http url>
```

### Paths do ini reconhecidos
```
localassets/firefall.ini   -> Ini local de assets
firefall.ini               -> Ini principal
hotfix.ini                 -> Hotfixes
videosettings.ini          -> Settings de video
preview.ini                -> Preview settings
```

---

## Fontes

- PIN (Pirate Intelligence Network): https://github.com/themeldingwars/PIN
- PIN Releases: https://github.com/themeldingwars/PIN/releases
- PIN README: https://github.com/themeldingwars/PIN/blob/master/README.md
- Firefall.cs (config): https://github.com/themeldingwars/PIN/blob/master/Lib/Shared.Web/Config/Firefall.cs
- SINner (emulador): https://github.com/themeldingwars/SINner
- Forum The Melding Wars: https://forums.themeldingwars.com/topic/31/pirate-intelligence-network

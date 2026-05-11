# LocalSend Improved — Diseño v1

**Fecha:** 2026-05-10
**Autor:** Diego Resendez (diego.resendez@zero-oneit.com)
**Estado:** Aprobado (brainstorming) → pendiente de revisión de spec y plan de implementación
**Nombre código:** *placeholder* (decisión de naming pendiente; este documento usa `<app>`)

---

## 1. Resumen ejecutivo

Daemon nativo en Rust para transferencia de archivos en LAN y WAN privado, orientado a **servidores, NAS y homelabs** — el nicho que LocalSend actual no cubre por estar diseñado primariamente para móvil/desktop con GUI.

El producto convive con el ecosistema LocalSend (compatibilidad bidireccional 100% con su protocolo v2) y añade un protocolo nativo propio sobre QUIC con TLS 1.3 (mutual auth vía certs autofirmados que embeben la pubkey Ed25519 del peer), identidades persistentes, resume real, descubrimiento WAN vía rendezvous self-hosted y extensibilidad estructurada.

Modelo dual: **AGPL-3.0 + DCO** para una base abierta plenamente funcional e irreversiblemente libre, **BSL** para crates VIP separadas dirigidas a uso comercial/empresarial (relay gestionado, sync de carpetas, políticas admin, audit log, SSO).

Roadmap v1 estimado en **~22 semanas** con 7 milestones shippable.

---

## 2. Visión y alcance

### 2.1 Para quién es v1

- Usuarios técnicos con NAS, Raspberry Pi, servidor casero o VPS.
- Equipos pequeños que quieren buzón de archivos compartido en oficina sin nube.
- Devs que necesitan scriptear envíos/recepciones desde CI o pipelines.

### 2.2 Para quién NO es v1

- Usuarios móviles puros (los cubre LocalSend original vía compat bidireccional).
- Reemplazo de Syncthing — no es sync continuo en v1.
- Reemplazo de Tailscale/WireGuard — el relay TURN-like no entra en v1.

### 2.3 Criterios de éxito v1

1. Un usuario instala el binario en su NAS y recibe un archivo desde su teléfono con LocalSend original **sin configurar nada** (mDNS just works).
2. Dos nodos del protocolo nativo se emparejan con un comando y a partir de ahí transfieren sin fricción.
3. Un script Python de ~10 líneas usa la API local para listar peers y enviar un archivo.
4. El daemon corre 7 días estable bajo carga normal sin leaks visibles.

### 2.4 Fuera de scope v1

- Crates VIP (relay managed, sync de carpeta, SSO/OIDC, audit log firmado, admin policies multi-usuario).
- Plugins WASM.
- Apps móviles nativas (Android/iOS).
- Bindings UniFFI **construidos** (la arquitectura los **prepara**).
- Relay TURN-like (hole punching sí, relay full no).
- Adaptadores extra de protocolos (WebDAV, SMB, FTP).

---

## 3. Arquitectura de alto nivel

```
┌────────────────────────────────────────────────────────────────┐
│                         DAEMON (binario único)                  │
│                                                                 │
│  ┌──────────────────────────────────────────────────────────┐  │
│  │                       CORE (crate)                        │  │
│  │  ├── identity   (Ed25519, fingerprints, trust store)      │  │
│  │  ├── protocol                                             │  │
│  │  │     ├── localsend_v2   (HTTP, compat 100%)             │  │
│  │  │     └── native_v1      (QUIC+TLS1.3+Ed25519, full)      │  │
│  │  ├── transfer   (chunking, resume, hashing BLAKE3)        │  │
│  │  ├── discovery  (mDNS + rendezvous)                       │  │
│  │  ├── storage    (inbox/outbox, manifest persistente)      │  │
│  │  └── policy     (auto-accept rules, quotas, allowlist)    │  │
│  │                                                           │  │
│  │  Sin I/O directo. Todo a través de traits inyectables.    │  │
│  └──────────────────────────────────────────────────────────┘  │
│                              ▲                                  │
│              ┌───────────────┼───────────────┐                  │
│  ┌───────────┴────┐  ┌───────┴────┐  ┌───────┴────────┐         │
│  │   Listeners    │  │  Local API │  │     Hooks      │         │
│  │  LocalSend v2  │  │  gRPC +    │  │  on_receive,   │         │
│  │  Native (QUIC) │  │  HTTP+SSE  │  │  on_send, etc. │         │
│  │  mDNS          │  │  + WebUI   │  │  (exec/webhook)│         │
│  └────────────────┘  └────────────┘  └────────────────┘         │
└────────────────────────────────────────────────────────────────┘
        ▲                    ▲                    ▲
        │                    │                    │
  Peers en LAN          Clientes locales      Scripts del usuario
  (LocalSend y          (CLI, TUI,            (webhooks, exec
   nuestros)             WebUI, GUI Tauri)     en eventos)
```

### 3.1 Principios estructurales

1. **`core` sin I/O, todo vía traits.** Recibe implementaciones inyectadas de `Storage`, `Network`, `Clock`, `IdentityVault`. Esto lo hace testeable, embebible (para UniFFI futuro), y reutilizable por terceros.
2. **El daemon es el único proceso con estado.** Todas las superficies (CLI, TUI, WebUI, GUI Tauri) son **clientes** de la API local. Cero reimplementación del protocolo en superficies.
3. **Listeners en paralelo, política unificada.** LocalSend v2 y protocolo nativo aceptan por separado pero comparten storage, policy y hooks.
4. **La API local es la frontera estable.** Versionada semánticamente. El core puede refactorizarse libremente; la API gRPC es el contrato externo.
5. **GUI dual: standalone o cliente remoto.** Detalle en §3.3.

### 3.2 Workspace Cargo

```
<app>/
├── core/                # libcore (AGPL-3.0)
├── protocol-localsend-v2/  # listener compat (AGPL-3.0)
├── protocol-native-v1/  # QUIC+TLS1.3+Ed25519 nativo (AGPL-3.0)
├── daemon/              # binario daemon (AGPL-3.0)
├── cli/                 # binario CLI (AGPL-3.0)
├── tui/                 # binario TUI con ratatui (AGPL-3.0)
├── webui/               # assets SPA embebidos (AGPL-3.0)
├── gui/                 # Tauri shell (AGPL-3.0)
├── rendezvous/          # binario rendezvous server (AGPL-3.0)
├── proto/               # .proto files versionados (AGPL-3.0)
└── sdks/
    ├── python/          # autogenerado
    ├── typescript/      # autogenerado
    └── go/              # autogenerado

<app>-pro/  (repo SEPARADO, licencia BSL)
├── vip-relay-managed/
├── vip-folder-sync/
├── vip-admin-policies/
├── vip-audit-log/
├── vip-sso/
└── daemon-pro/          # binario distinto que linka VIP crates
```

### 3.3 GUI: standalone vs cliente remoto

La GUI Tauri puede arrancar en dos modos, decididos al iniciar:

- **Modo standalone**: si no detecta un daemon local corriendo, levanta `libcore` in-process y expone la API solo a sí misma. Comportamiento idéntico a un daemon más una GUI conectada localmente. Un usuario en macOS/Windows que solo quiere usar la app obtiene una experiencia "todo-en-uno" sin saber que existe un daemon.
- **Modo cliente remoto**: el usuario decide conectar a un daemon en otra máquina (típicamente su NAS). La GUI cierra el libcore in-process y se convierte en cliente puro autenticándose con la pubkey Ed25519 del peer remoto (no con bearer token de loopback).

Implicación: **una sola implementación del comportamiento** vive en `core`. El binario GUI bundlea `libcore` completo (~30-50 MB vs ~5-15 MB del daemon solo).

### 3.4 "Mesh" sin servidor central

Cada nodo se anuncia y descubre a otros por mDNS (LAN) o por rendezvous self-hosted (WAN). No hay servidor central obligatorio. Esto **sale del diseño**, no requiere componente adicional.

---

## 4. Identidad, descubrimiento y protocolos

### 4.1 Identidad

- Par de claves **Ed25519** generado al primer arranque.
- **Storage**:
  - Linux: `~/.config/<app>/identity.key` (`0600`)
  - macOS: `~/Library/Application Support/<app>/identity.key`
  - Windows: `%APPDATA%\<app>\identity.key`
  - NAS/servidor sin home: `/var/lib/<app>/identity.key`
- **Identidad pública en wire**: `pubkey` base64.
- **Fingerprint humano**: `SHA-256(pubkey)` truncado a 64 bits, formato `a1b2-c3d4-e5f6-7890`.
- **Rotación**: vía CLI (`identity rotate`), invalida todas las relaciones de confianza existentes.

### 4.2 Trust store (SQLite)

Archivo `trust.db` con tabla `peers`:

| campo | tipo | propósito |
|---|---|---|
| `fingerprint` | TEXT PK | clave humana |
| `pubkey` | BLOB | clave Ed25519 cruda |
| `label` | TEXT | nombre dado por el usuario |
| `trusted_at` | INTEGER | unix timestamp del pairing |
| `last_seen` | INTEGER | última vez visto |
| `policy` | TEXT | `auto_accept` / `prompt` / `block` |

Justificación de SQLite frente a LMDB: inspectability (`sqlite3 trust.db ...`) crítica en entornos homelab, queries flexibles para TUI/WebUI, volúmenes pequeños no justifican la complejidad de un KV puro. Si en benchmarks aparece cuello en hot path de transferencias activas, hay salida vía `redb` (pure Rust embedded KV) sin tocar el resto. **No pre-optimizar.**

### 4.3 Pairing del protocolo nativo

Modo confiable (relación persistente):

1. Ambos lados ejecutan `pair` (CLI/GUI).
2. Cada lado anuncia pubkey en LAN vía discovery.
3. Usuario en lado A selecciona a B.
4. Ambos lados muestran un **SAS de 6 dígitos** derivado de ambas pubkeys; el usuario verifica que coinciden (defensa MITM en primer encuentro).
5. Confirmación → ambas pubkeys quedan en el `trust.db` del otro con `policy=auto_accept`.

Modos de pairing soportados:
- **SAS de 6 dígitos** para terminal/headless.
- **QR code** que codifica `{pubkey, endpoint, nonce, sas_expected}` para móvil/escaneo cruzado.

Modo invitado (one-off): flujo PIN efímero estilo LocalSend, sin tocar el trust store.

### 4.4 Descubrimiento

**LAN — mDNS dual:**
- `_localsend._tcp` puerto 53317 (TXT idéntico a LocalSend para compat).
- `_<app>._udp` con TXT que anuncia capacidades QUIC + fingerprint.

Ambos services se anuncian sobre el mismo socket mDNS estándar (`5353/udp` multicast); son service types distintos, no listeners separados. Un nodo que ve ambos records sabe que puede usar el protocolo nativo. Solo `_localsend._tcp` → cae a compat.

**WAN — rendezvous self-hosted, cero defaults:**

```
   Peer A (NAT)                  Rendezvous                Peer B (NAT)
       │                              │                          │
       │── register(pubkey, endpoint) ─►                          │
       │                              ◄─ register(pubkey, ep) ──│
       │── lookup(pubkey_of_B) ──────►                            │
       │◄──── B's endpoint + ICE candidates ─────                 │
       │                              ────► relay B's view ──────│
       │                                                          │
       │◄═══════════ hole punching (QUIC) ═══════════════════════►│
       │              (rendezvous se sale del medio)              │
```

- **No hay rendezvous oficial por defecto** en v1. El usuario configura el suyo o vive en LAN. La opción gestionada por el proyecto entra como parte de la oferta VIP cuando exista monetización.
- El rendezvous se distribuye como binario aparte (otra crate, mismo workspace) que el usuario puede correr en cualquier VPS.
- El rendezvous solo conoce `pubkey → endpoint`. No ve tráfico ni metadata del archivo.
- Sin relay TURN-like en v1. Si hole punching falla por CGNAT, la conexión falla y el usuario lo sabe.

### 4.5 Protocolo LocalSend v2 (listener compat)

Implementación tal cual del protocolo HTTP de LocalSend, sin invenciones:

- `GET /api/localsend/v2/info` — device info.
- `POST /api/localsend/v2/prepare-upload` — anuncio de archivos, respuesta con `sessionId` y mapping de tokens.
- `POST /api/localsend/v2/upload?sessionId=&fileId=&token=` — bytes del archivo.
- `POST /api/localsend/v2/cancel?sessionId=` — cancelación.
- TLS self-signed con cert generado al vuelo, fingerprint en el TXT de mDNS.

Política frente a v2:
- Los archivos recibidos por este listener entran al mismo `storage/inbox` y disparan los mismos hooks que los del nativo.
- Limitaciones aceptadas (no se intenta extender el protocolo en esta ruta):
  - Sin resume real (LocalSend reenvía desde cero).
  - Sin E2E con identidades persistentes — solo TLS self-signed.
  - Sin extensiones propias.

### 4.6 Protocolo nativo v1

**Transport:** QUIC vía `quinn`, puerto configurable (default `53400/udp`).

**Por qué QUIC:** multiplexing de streams, connection migration (Wi-Fi → móvil sin romper sesión), 0-RTT en reconexiones, NAT traversal amigable, TLS 1.3 incluido.

**Auth encima de QUIC:** TLS 1.3 configurado con cert autofirmado que embebe la pubkey Ed25519. Validación contra trust store local (no contra CAs del sistema). Para invitados efímeros, cert temporal validado por SAS humano.

**Streams:**

- **Stream 0 (control)**: handshake con negociación de versión + lista de extensiones soportadas (`resume`, `e2e_identity`, `sync_folder`, etc.). Ambos lados eligen la **intersección**. Mensajes en protobuf.
- **Stream N (data, uno por archivo)**: chunks con offset, hash incremental BLAKE3, permite **resume real**.

**Negociación de extensiones:** es la clave del modelo open source / VIP. La extensión "compresión Zstd" es comunitaria; la extensión "sync de carpeta bidireccional" puede ser VIP — el daemon comunitario simplemente no la anuncia, así que el peer comunitario nunca espera esa capacidad.

### 4.7 Puertos

| Puerto | Protocolo | Para qué |
|---|---|---|
| `53317/tcp` | LocalSend v2 (HTTP+TLS self-signed) | Compat |
| `53317/udp` | mDNS LocalSend | Discovery compat |
| `53400/udp` | QUIC (protocolo nativo) | Transferencias nativas |
| `5353/udp` | mDNS estándar | Discovery nativo |
| `localhost:53500/tcp` | gRPC API local | Solo loopback |
| `localhost:53501/tcp` | HTTP+SSE API local + WebUI | Solo loopback |
| `localhost:53502/tcp` | Métricas Prometheus | Solo loopback |

Todos configurables vía archivo o flags.

---

## 5. API local del daemon

### 5.1 Transporte

- **Primario**: gRPC + protobuf en `localhost:53500`.
- **Mirror**: HTTP/JSON + Server-Sent Events en `localhost:53501` (para herramientas que prefieren curl/fetch y para la WebUI embebida).
- Ambos versionados (`/v1/…`). El `.proto` es el contrato canónico desde el que se generan los SDKs.

### 5.2 Autenticación

- **Loopback**: token bearer almacenado en `~/.config/<app>/api.token` (perms `0600`). Cualquier proceso del mismo usuario puede leerlo. Solo expuesto en `localhost`.
- **GUI remota**: se autentica como peer del protocolo nativo (pubkey Ed25519), **no con bearer token**. La GUI remota se conecta al puerto QUIC nativo del daemon (`53400/udp`), realiza el handshake TLS 1.3 mutuo con su cert Ed25519, y dentro de la sesión cifrada abre streams gRPC dedicados a la API local. Es decir: **la API local del daemon viaja por loopback en plain TCP O por QUIC cifrado cuando el cliente es remoto**, pero el conjunto de servicios gRPC es idéntico. El daemon **nunca expone los puertos `53500/tcp` o `53501/tcp` fuera de loopback**.

### 5.3 Servicios gRPC mínimos v1

```
service Peers {
  rpc List(...) returns (PeerList);
  rpc Get(PeerId) returns (Peer);
  rpc Trust(PeerId) returns (Peer);
  rpc Untrust(PeerId) returns (Empty);
  rpc Pair(PairRequest) returns (stream PairEvent);
  rpc Rename(RenameRequest) returns (Peer);
}

service Transfers {
  rpc Send(SendRequest) returns (stream TransferEvent);
  rpc ReceiveAccept(SessionId) returns (Empty);
  rpc ReceiveReject(SessionId) returns (Empty);
  rpc Cancel(SessionId) returns (Empty);
  rpc ListActive(...) returns (TransferList);
  rpc ListHistory(HistoryQuery) returns (TransferList);
  rpc Resume(SessionId) returns (stream TransferEvent);
}

service Inbox {
  rpc List(InboxQuery) returns (InboxList);
  rpc GetFile(FileId) returns (stream FileBytes);
  rpc Delete(FileId) returns (Empty);
  rpc MarkRead(FileId) returns (Empty);
}

service Discovery {
  rpc ScanLan(...) returns (PeerCandidates);
  rpc LookupWan(LookupRequest) returns (PeerCandidates);
}

service Daemon {
  rpc Info(...) returns (DaemonInfo);
  rpc ConfigGet(ConfigKey) returns (ConfigValue);
  rpc ConfigSet(ConfigKV) returns (Empty);
  rpc LogsTail(LogQuery) returns (stream LogEntry);
}

service Events {
  rpc Subscribe(EventFilter) returns (stream Event);
}
```

CLI, TUI, WebUI y GUI son consumidores idénticos de estos servicios.

---

## 6. Modelo open source / VIP

### 6.1 Tres ejes separados

**Eje 1 — Licencia del código:**
- **Base abierta** (core, protocolos, daemon, CLI, TUI, WebUI, GUI Tauri, SDKs, rendezvous): **AGPL-3.0**.
- **Crates VIP** (en repo separado): **BSL 1.1** o similar — uso comercial/empresarial requiere clave de licencia.

**Eje 2 — Modelo de contribución:**
- **DCO (Developer Certificate of Origin), NO CLA.** Los contribuidores firman commits (`Signed-off-by`) pero conservan copyright. Esto hace **imposible relicenciar la base abierta en el futuro** — ni siquiera el dueño puede hacer un "rug pull" estilo MongoDB/Elastic. La promesa "siempre open source" pasa de ser un compromiso a ser una imposibilidad legal.
- Las crates VIP, al vivir en repo separado con copyright único o contribuidores bajo contrato específico, sí pueden mantener su licencia BSL.

**Eje 3 — Empaquetado de features:**
- **Cero `cfg` features ocultas** en el código abierto. Si una feature es VIP, vive en una crate VIP separada en otro repo.
- El daemon abierto **arranca sin ninguna crate VIP**, completamente funcional para los casos personal/homelab/equipo pequeño.
- La build VIP es **un daemon distinto** (`<app>-pro`) que linka las crates VIP. Mismo nombre resoluble, contenidos distintos.

### 6.2 Gating de licencia VIP

- El daemon-pro arranca con un archivo `license.key` (clave Ed25519 firmada por el proveedor).
- Sin licencia válida → arranca en modo libre equivalente al daemon abierto.
- **No phone-home por defecto**. La clave se verifica localmente con la pubkey del proveedor embebida en el binario.
- Renovación: nueva `license.key` reemplaza la vieja.

### 6.3 Candidatos VIP iniciales

- **Relay managed**: relay TURN-like alojado por el proveedor, con SLA y bandwidth gestionado.
- **Folder sync**: sync de carpeta bidireccional Syncthing-like.
- **Admin policies**: allowlists granulares, quotas por peer, roles multi-usuario.
- **Audit log firmado**: append-only, firmado para cumplimiento.
- **SSO/OIDC**: acceso a WebUI con identidades empresariales.

### 6.4 Compromiso de marca — qué NUNCA es VIP

- Protocolo, criptografía, identidad, descubrimiento, transferencia core.
- CLI/TUI/WebUI básicas.
- API local completa.
- Compat LocalSend v2.

Quien quiera "LocalSend mejorado en mi NAS" lo tiene gratis para siempre. VIP es solo para necesidades empresariales/avanzadas.

---

## 7. Persistencia y hooks

### 7.1 Layout de filesystem

```
Linux (XDG):
  ~/.config/<app>/         # identity.key, api.token, config.toml
  ~/.local/share/<app>/    # trust.db, manifests.db, logs/
  ~/Downloads/<app>/       # inbox por defecto (configurable)

macOS:
  ~/Library/Application Support/<app>/      # config + state (identity, trust.db, manifests.db)
  ~/Library/Logs/<app>/                     # logs
  ~/Downloads/<app>/                        # inbox por defecto

Windows:
  %APPDATA%\<app>\                          # config + state
  %LOCALAPPDATA%\<app>\Logs\                # logs
  %USERPROFILE%\Downloads\<app>\            # inbox por defecto

Servidor/NAS sin $HOME:
  /etc/<app>/                               # config
  /var/lib/<app>/                           # state (identity, trust.db, manifests.db)
  /var/log/<app>/                           # logs
  /var/lib/<app>/inbox/                     # inbox por defecto
```

### 7.2 Lifecycle de inbox

1. Recibido → escrito a `inbox/incoming/<sessionId>/<file>` (carpeta temporal por sesión).
2. Verificación BLAKE3 OK → rename atómico a `inbox/<peer_label>/<file>` o destino configurado.
3. Manifest queda en `manifests.db` con estado (`completed`, `failed`, `cancelled`).
4. GC opcional: archivos no leídos más allá de N días → notificación o auto-delete (configurable).

### 7.3 Quotas

- Configurables por peer y globales.
- Si un peer no confiable intenta superar quota → reject con código de error específico.

### 7.4 Hooks

Dos tipos en v1:

**Webhook (HTTP POST):**
- JSON con metadata al endpoint configurado por el usuario.
- Eventos: `transfer.started`, `transfer.completed`, `transfer.failed`, `transfer.cancelled`, `peer.paired`, `peer.untrusted`, `inbox.gc_run`.
- Firma HMAC opcional en headers (`X-<app>-Signature: sha256=...`).

**Exec local:**
- Daemon spawnea un script con variables de entorno (`<APP>_EVENT`, `<APP>_PEER`, `<APP>_FILE_PATH`, etc.).
- Timeout estricto (5s default, configurable).
- Sandbox básico: ejecutado como usuario sin privs adicionales; en Linux opcionalmente con `unshare` para aislar red/PID/mount.
- Los hooks son disparados **igual** por eventos del listener LocalSend v2 y del nativo.

Configuración ejemplo:

```toml
[[hooks]]
event  = "transfer.completed"
type   = "exec"
script = "/usr/local/bin/notify-discord.sh"

[[hooks]]
event   = "peer.paired"
type    = "webhook"
url     = "https://my-homelab.example/<app>"
secret  = "${ENV_SIGNING_SECRET}"
```

### 7.5 Observabilidad

- **Logs estructurados JSON** a stdout (systemd/Docker friendly) + archivo rotativo opcional.
- **Métricas Prometheus** en `localhost:53502/metrics`: transferencias activas, bytes/s, peers conocidos, errores por código.
- **Healthcheck**: `GET /healthz` y `GET /readyz` en el HTTP API.
- **Trace IDs** correlacionables entre logs y eventos webhook.

---

## 8. Extensibilidad (Nivel 0 + Nivel 1)

### 8.1 Nivel 0 — Crate Rust publicable

El crate `core` se publica en crates.io bajo AGPL-3.0. Cualquier proyecto Rust puede consumirlo: bots, scripts, integraciones, herramientas alternativas.

**Disciplina al diseñar:** `core` no debe exponer en su API pública:
- Generics opacos con bounds complejos en límites de módulo.
- Lifetimes problemáticos.
- Async traits sin objeto seguro.

Esta disciplina mantiene **abierta** la puerta a UniFFI (Nivel 2) en el futuro sin refactor doloroso. UniFFI **no se construye en v1**, pero la API pública del core respeta sus restricciones.

### 8.2 Nivel 1 — SDKs autogenerados

Desde el `.proto` versionado, se generan SDKs en:

- **Python** (vía `betterproto` o `grpclib`).
- **TypeScript** (vía `ts-proto`).
- **Go** (vía `protoc-gen-go-grpc`).

Publicados en sus respectivos package managers (PyPI, npm, Go modules) bajo AGPL-3.0.

**Compromiso de estabilidad:** API marcada `v1` no rompe en minors. Cambios incompatibles requieren `v2`.

### 8.3 Niveles 2+ (fuera de v1, documentados)

- **Nivel 2 — UniFFI**: bindings idiomáticos para Swift/Kotlin/Python desde IDL Rust. Apuntado para v2 cuando entren las apps móviles.
- **Nivel 3 — C-ABI**: descartado hasta que haya demanda real concreta. El costo de mantener dos APIs (linda en Rust, fea en C) no se justifica en abstracto.
- **Nivel 4 — Plugins WASM en el daemon**: postergado a v2/v3. La arquitectura **reserva puntos de extensión** (`hooks/` con interfaz limpia) para que la adición sea no-traumática.

---

## 9. Testing

### 9.1 Tres capas

1. **Unitarias en `core`**: el core no toca I/O, se testea con `MockNetwork`, `MockStorage`, `MockClock`. **Coverage objetivo 80%+**, énfasis en handshakes, negociación de extensiones, resume, autenticación.
2. **Integración con el daemon real**:
   - Dos instancias en localhost con puertos distintos.
   - **Tests interop contra LocalSend real**: en CI, descargar el binario LocalSend oficial y validar envío/recepción bidireccional. **No opcional** — la compat v2 vive o muere por estas pruebas.
3. **Property-based testing**: `proptest` sobre el manifest parser, las extensiones negociadas, los rangos de resume.

### 9.2 Fuzzing

`cargo-fuzz` sobre los parsers de mensajes nativos para inputs de wire.

### 9.3 Soak tests

Job nocturno: dos daemons envían archivos random durante 8h. Detecta leaks de memoria/FDs/conexiones.

---

## 10. Entrega y packaging

### 10.1 Targets v1

| Plataforma | Formato | Notas |
|---|---|---|
| Linux x86_64 | binario estático (musl) + `.deb` + `.rpm` | systemd unit incluido |
| Linux aarch64 | binario estático (musl) + `.deb` | Raspberry Pi, NAS ARM |
| macOS x86_64 + aarch64 | universal binary + launchd plist + `.pkg` | firmado y notarizado |
| Windows x86_64 | `.exe` + servicio Windows + `.msi` | construido con `wix-rs` |
| Docker | imagen `distroless` o `scratch` | <30 MB |
| Homebrew | tap propio | macOS/Linux |
| NixOS | módulo | comunidad homelab |

GUI Tauri: distribución aparte con `.dmg`, `.AppImage`, `.msi`.

### 10.2 CI/CD

- GitHub Actions con `cross` o `zigbuild` para cross-compilación.
- Release automático en push de tag: builds firmados, checksums (cosign opcional), changelog automático.
- SBOM (CycloneDX) en cada release.

---

## 11. Roadmap

7 milestones shippable, cada uno con valor independiente.

| Milestone | Semanas | Entregable |
|---|---|---|
| **M0 — Esqueleto** | 1-2 | Workspace + CI + identidad Ed25519 + trust store + CLI mínimo |
| **M1 — Compat LocalSend v2** | 3-5 | Listener v2 + mDNS dual + inbox + CLI `peers`/`receive`. Demo: recibir archivo desde LocalSend Android en Raspberry Pi |
| **M2 — Protocolo nativo LAN** | 6-9 | QUIC + TLS 1.3 mutuo con certs Ed25519 + pairing SAS+QR + negociación de extensiones + transferencia con resume |
| **M3 — API local + TUI + WebUI** | 10-12 | gRPC+HTTP+SSE + SDKs autogenerados + TUI ratatui + WebUI embebida |
| **M4 — Hooks y observabilidad** | 13-14 | Webhooks + exec hooks + métricas + logs + packaging multi-plataforma |
| **M5 — WAN** | 15-17 | Rendezvous server + hole punching QUIC+STUN. Sin relay full |
| **M6 — GUI Tauri** | 18-20 | Modo standalone (`libcore` in-process) + modo cliente remoto. Reusa frontend de WebUI |
| **M7 — Endurecimiento y 1.0** | 21-22 | Soak tests 7d + interop verde + docs + audit invite + anuncio público |

**Total v1 estimado: ~22 semanas (5 meses)** con un dev full-time. Con dos devs paralelos desde M3 → ~3-4 meses.

---

## 12. Riesgos y mitigaciones

| Riesgo | Mitigación |
|---|---|
| Interop con LocalSend rompe entre versiones de LocalSend | Tests automatizados contra LocalSend real en CI, ejecutados en cada PR |
| QUIC bloqueado en redes corporativas con firewalls agresivos | Fallback documentado a TCP+TLS planeado para v1.x; v1 acepta el riesgo en LAN doméstica |
| Filtración accidental de código VIP al repo abierto | Crates separadas, **repos separados**, builds separados; review obligatorio para PRs cross-repo |
| Bug en crypto = brecha seria | Audit externo antes de 1.0 (presupuestar ~$15-25k); hasta entonces, comunicar producto como "beta de seguridad" |
| Adopción comunitaria lenta | La compat LocalSend v2 baja la barrera de entrada a cero — quien tenga LocalSend ya nos puede usar |
| Curva de Rust frustra contribución comunitaria | Documentación de arquitectura clara, módulos pequeños, ejemplos abundantes en docs |

---

## 13. Decisiones explícitamente NO tomadas

Para evitar parálisis en el siguiente paso (plan de implementación), estos puntos quedan **abiertos** y se resolverán durante la planificación o más adelante:

- **Nombre del proyecto.** Placeholder `<app>` en todo el documento.
- **Versiones exactas** de dependencias críticas (`quinn`, `rustls`, `tokio`, etc.) — quedan al plan.
- **Convención de error codes** en el wire protocol nativo — detalle de plan, no de diseño.
- **Mecanismo exacto de firma HMAC en webhooks** (algoritmo, header name) — convención estándar a fijar en plan.
- **Tap concreto de Homebrew, repo de NixOS** — operativo, no de diseño.
- **Presupuesto y proveedor del audit externo de crypto** — decisión de negocio antes de 1.0.

---

## 14. Glosario

- **AGPL-3.0**: GNU Affero General Public License. Copyleft fuerte que extiende GPL a uso "como servicio" — si modificas el código y lo expones por red, debes liberar la modificación.
- **BLAKE3**: función hash criptográfica moderna, rápida, con soporte nativo para hashing incremental.
- **BSL (Business Source License)**: licencia source-available con cláusula de "uso comercial requiere clave"; convierte en open source después de un periodo (típicamente 4 años).
- **DCO (Developer Certificate of Origin)**: firma `Signed-off-by` en commits donde el contribuidor afirma que tiene derecho de aportar bajo la licencia del proyecto; conserva copyright.
- **Ed25519**: esquema de firma digital de curva elíptica, rápido y con claves cortas (32 bytes).
- **Hole punching**: técnica de NAT traversal donde dos peers detrás de NAT inician conexiones simultáneas que abren un agujero en sus respectivos firewalls.
- **mDNS**: Multicast DNS, descubrimiento de servicios en LAN sin servidor.
- **QUIC**: transporte sobre UDP con TLS 1.3 integrado, multiplexing nativo y connection migration.
- **SAS (Short Authentication String)**: cadena corta derivada criptográficamente de ambas pubkeys en un handshake; usuarios la comparan visual/auralmente para detectar MITM.
- **STUN**: protocolo para que un peer descubra su endpoint público visto desde Internet.
- **UniFFI**: generador de bindings de Mozilla para exponer Rust a Swift/Kotlin/Python/Ruby con IDL.

---

*Fin del documento.*

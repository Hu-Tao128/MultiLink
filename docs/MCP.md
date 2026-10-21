# MCP en MultiLink

Guía práctica para configurar y usar servidores **MCP (Model Context Protocol)** en MultiLink.

> **Estado actual (importante)**
>
> MultiLink ya incluye:
>
> - Un **editor de configuración MCP** en `Configuración → Servidores MCP`.
> - Persistencia de esa configuración en `multilink.toml` bajo `[mcp]`.
> - Un **adaptador MCP delgado** (`core/src/mcp_adapter.rs`) que traduce llamadas
>   tipo MCP a envelopes del protocolo LAN de MultiLink y viceversa.
>
> Todavía **no** se lanzan procesos MCP locales por `stdio` desde la app ni se
> inyectan automáticamente sus tools en el bucle del agente. Es decir: la UI
> registra y guarda los servidores, y el adaptador define el contrato de
> herramientas, pero la ejecución completa de servidores MCP externos es trabajo
> pendiente (ver `docs/ROADMAP_V3_STATE.md`). Esta guía documenta lo que funciona
> hoy y cómo preparar la configuración para cuando se conecte el runtime.

---

## 1. ¿Qué es MCP?

MCP es un protocolo abierto para que un modelo o agente acceda a **herramientas y
datos externos** de forma estandarizada. Un servidor MCP expone:

- **Tools**: funciones que el modelo puede invocar (leer archivos, consultar una
  base de datos, buscar en la web, etc.).
- **Resources**: datos de solo lectura (archivos, registros, documentos).
- **Prompts**: plantillas reutilizables.

MultiLink usa el mismo formato de configuración que clientes como OpenCode o
Claude Desktop: un comando ejecutable, sus argumentos y variables de entorno.

---

## 2. Configurar servidores MCP en la GUI

1. Abre **Configuración** (icono de ajustes en la barra lateral).
2. Baja hasta **Servidores MCP**.
3. Pulsa **Abrir editor MCP**.
4. Escribe o pega un **arreglo JSON** con tus servidores.
5. Pulsa **Guardar**.

El editor valida el JSON y lo persiste en:

| Sistema | Ruta habitual |
|---|---|
| Linux | `~/.config/multilink/multilink.toml` |
| macOS | `~/Library/Application Support/multilink/multilink.toml` |
| Windows | `%APPDATA%\multilink\multilink.toml` |

Si ya existía un `config.toml` antiguo, MultiLink lo sigue usando como fallback.

### Formato JSON del editor

El editor espera una **lista de objetos**. Cada objeto tiene esta forma:

```json
[
  {
    "name": "filesystem",
    "command": "npx",
    "args": ["-y", "@modelcontextprotocol/server-filesystem", "/home/tu-usuario/proyectos"],
    "env": {},
    "enabled": true
  }
]
```

| Campo | Tipo | Obligatorio | Descripción |
|---|---|---|---|
| `name` | string | Sí | Nombre único y descriptivo del servidor. |
| `command` | string | Sí | Ejecutable a lanzar (`npx`, `uvx`, `python`, `node`, ruta absoluta...). |
| `args` | array de strings | No | Argumentos del comando. |
| `env` | objeto string → string | No | Variables de entorno para el proceso. |
| `enabled` | bool | No | `true` por defecto. Un servidor deshabilitado no debería arrancarse. |

> El editor **no** acepta comentarios. Si necesitas documentar algo, usa el campo
> `name` o guarda tus notas fuera del JSON.

---

## 3. Ejemplos listos para copiar

### 3.1 Filesystem (acceso controlado a una carpeta)

```json
[
  {
    "name": "filesystem-proyectos",
    "command": "npx",
    "args": [
      "-y",
      "@modelcontextprotocol/server-filesystem",
      "/home/ana/proyectos"
    ],
    "env": {},
    "enabled": true
  }
]
```

Da acceso de lectura/escritura **solo** a la carpeta indicada. Usa rutas
absolutas y evita pasar `/`, `~` o el home completo.

### 3.2 Memoria persistente

```json
[
  {
    "name": "memory",
    "command": "npx",
    "args": ["-y", "@modelcontextprotocol/server-memory"],
    "env": {
      "MEMORY_FILE_PATH": "/home/ana/.local/share/multilink/mcp-memory.json"
    },
    "enabled": true
  }
]
```

### 3.3 SQLite (base de datos local)

```json
[
  {
    "name": "sqlite-analytics",
    "command": "uvx",
    "args": [
      "mcp-server-sqlite",
      "--db-path",
      "/home/ana/datos/analytics.db"
    ],
    "env": {},
    "enabled": true
  }
]
```

`uvx` requiere `uv` instalado (`pip install uv`). También puedes usar
`python -m mcp_server_sqlite` si prefieres un entorno virtual propio.

### 3.4 Varios servidores a la vez

```json
[
  {
    "name": "filesystem-proyectos",
    "command": "npx",
    "args": ["-y", "@modelcontextprotocol/server-filesystem", "/home/ana/proyectos"],
    "env": {},
    "enabled": true
  },
  {
    "name": "github",
    "command": "npx",
    "args": ["-y", "@modelcontextprotocol/server-github"],
    "env": {
      "GITHUB_PERSONAL_ACCESS_TOKEN": "ghp_reemplaza_esto"
    },
    "enabled": false
  }
]
```

> Evita dejar tokens reales en el archivo si lo vas a compartir o versionar.
> Prefiere variables de entorno del sistema o un gestor de secretos.

---

## 4. Equivalente en `multilink.toml`

La GUI escribe la misma información en TOML. Puedes editarla a mano cuando la app
esté cerrada:

```toml
[mcp]

[[mcp.servers]]
name = "filesystem-proyectos"
command = "npx"
args = ["-y", "@modelcontextprotocol/server-filesystem", "/home/ana/proyectos"]
enabled = true

[mcp.servers.env]
```

Con variables de entorno:

```toml
[[mcp.servers]]
name = "github"
command = "npx"
args = ["-y", "@modelcontextprotocol/server-github"]
enabled = true

[mcp.servers.env]
GITHUB_PERSONAL_ACCESS_TOKEN = "ghp_..."
```

El campo `env` es opcional; si no lo necesitas, omítelo.

---

## 5. Diferencias con el formato de OpenCode / Claude Desktop

MultiLink usa una **lista** de servidores. OpenCode y Claude Desktop suelen usar
un **objeto indexado por nombre**. La conversión es directa:

**OpenCode / Claude Desktop**

```json
{
  "mcp": {
    "filesystem": {
      "type": "local",
      "command": ["npx", "-y", "@modelcontextprotocol/server-filesystem", "/tmp"],
      "enabled": true
    }
  }
}
```

**MultiLink**

```json
[
  {
    "name": "filesystem",
    "command": "npx",
    "args": ["-y", "@modelcontextprotocol/server-filesystem", "/tmp"],
    "env": {},
    "enabled": true
  }
]
```

Reglas de conversión:

1. La clave del objeto pasa a `name`.
2. El primer elemento de `command` es `command`; el resto pasa a `args`.
3. `environment` (Claude) o `env` (OpenCode) pasa a `env`.
4. `enabled` se conserva.

---

## 6. Cómo se conecta MCP con MultiLink

### 6.1 Capa de configuración

`core/src/config.rs` define:

- `McpServerConfig`: `name`, `command`, `args`, `env`, `enabled`.
- `McpConfig`: contiene `servers: Vec<McpServerConfig>`.

El bridge (`gui/rust/chat_controller`) expone `mcpConfigJson()` y
`saveMcpConfigJson()` para que la GUI lea y guarde la lista sin duplicar lógica
en QML.

### 6.2 Adaptador MCP ↔ LAN

`core/src/mcp_adapter.rs` implementa `ThinMcpAdapter`, que traduce llamadas de
herramienta a los mensajes del **LAN Agent**:

| Tool MCP | Envelope LAN | Uso |
|---|---|---|
| `chat.dispatch` | `Dispatch` | Enviar un prompt a una sesión. |
| `chat.ping` | `Ping` | Comprobar conectividad. |
| `tools.list` | `ToolList` | Listar herramientas disponibles. |
| `tools.execute` | `ToolExecute` | Ejecutar una tool con argumentos. |

Ejemplo de llamada que el adaptador entiende:

```json
{
  "id": "req-1",
  "tool": "tools.execute",
  "arguments": {
    "tool_name": "fs_ls",
    "arguments": { "path": "." }
  }
}
```

La respuesta se normaliza a:

```json
{
  "id": "req-1",
  "result": {
    "content": [{ "type": "json", "json": { "entries": [] } }]
  },
  "error": null
}
```

### 6.3 Qué falta para ejecutar servidores MCP externos

El siguiente paso pendiente es un cliente MCP local que:

1. Lea `[mcp].servers` al iniciar.
2. Lance cada servidor habilitado por `stdio`.
3. Haga el handshake `initialize` y `tools/list`.
4. Registre esas tools en el registro del agente.
5. Cierre los procesos de forma ordenada al salir.

Hasta que eso exista, el editor MCP sirve para **preparar y versionar** la
configuración, y el adaptador LAN cubre los casos de dispatch y tools expuestas
por MultiLink.

---

## 7. Buenas prácticas de seguridad

- **Mínimo privilegio**: concede solo carpetas y credenciales necesarias.
- **Rutas absolutas**: evita `~`, `.` y variables sin expandir.
- **Sin secretos en JSON compartido**: usa `env` del sistema o un gestor de
  secretos; nunca subas tokens a Git.
- **Revisa servidores de terceros**: `npx -y` descarga y ejecuta el paquete en
  cada arranque. Fija versiones (`@modelcontextprotocol/server-filesystem@1.2.3`)
  si necesitas reproducibilidad.
- **Deshabilita lo que no uses**: `"enabled": false` reduce superficie de ataque
  y tiempo de arranque.
- **Separa entornos**: no apuntes servidores MCP de pruebas a bases de datos de
  producción.
- **Audita argumentos peligrosos**: un servidor MCP con acceso a shell puede
  ejecutar comandos arbitrarios. Trátalo como código de confianza.

---

## 8. Solución de problemas

| Síntoma | Causa probable | Solución |
|---|---|---|
| `JSON invalid` al guardar | Coma final, comillas simples o comentarios | Valida el JSON con `jq . archivo.json` y vuelve a pegar. |
| `command not found` | El ejecutable no está en el `PATH` de la app | Usa ruta absoluta (`/usr/bin/node`, `/home/ana/.local/bin/uvx`). |
| `npx` tarda mucho | Descarga el paquete en cada arranque | Instala el servidor globalmente o fija una versión en caché. |
| `Permission denied` | Falta permiso de ejecución o carpeta protegida | Revisa `chmod +x` y los permisos de la carpeta indicada. |
| El servidor no aparece en el chat | El runtime MCP aún no inicia procesos | Es el comportamiento actual; usa la configuración para clientes MCP compatibles mientras se completa la integración. |
| Variables de entorno ignoradas | `env` mal anidado en TOML | Debe ser `[mcp.servers.env]` justo después del servidor. |
| Duplicados o nombres confusos | Dos servidores con el mismo `name` | Usa nombres únicos: `filesystem-proyectos`, `filesystem-docs`. |

Para verificar que la configuración quedó guardada:

```bash
grep -A 20 '^\[mcp\]' ~/.config/multilink/multilink.toml
```

O desde la app: **Configuración → Servidores MCP → Abrir editor MCP** (debe
mostrar exactamente lo guardado).

---

## 9. Probar un servidor MCP fuera de MultiLink

Mientras la integración de runtime está pendiente, puedes validar que tu
configuración es correcta con cualquier cliente MCP compatible:

- **OpenCode**: añade el servidor a su configuración `mcp` y ejecuta
  `opencode` para comprobar que lista sus tools.
- **Claude Desktop**: usa `claude_desktop_config.json` con el mismo comando.
- **MCP Inspector**:

  ```bash
  npx @modelcontextprotocol/inspector npx -y @modelcontextprotocol/server-filesystem /tmp
  ```

  El inspector abre una UI web donde puedes ver tools, resources y ejecutar
  llamadas de prueba.

Si el servidor responde correctamente en el inspector, la configuración
(`command`, `args`, `env`) es válida para MultiLink.

---

## 10. Referencias

- Especificación MCP: <https://modelcontextprotocol.io>
- Servidores oficiales: <https://github.com/modelcontextprotocol/servers>
- Adaptador LAN de MultiLink: `core/src/lan_agent.rs`
- Adaptador MCP delgado: `core/src/mcp_adapter.rs`
- Configuración: `core/src/config.rs`
- Roadmap: `docs/ROADMAP_V3_STATE.md`

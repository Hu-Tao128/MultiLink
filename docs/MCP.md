# MCP en MultiLink

Guía práctica para configurar y usar servidores **MCP (Model Context Protocol)** en MultiLink.

> **Estado actual**
>
> MultiLink ya incluye:
>
> - Un **editor de configuración MCP** en `Configuración → Servidores MCP`.
> - Persistencia de esa configuración en `multilink.toml` bajo `[mcp]`.
> - Un **cliente MCP por `stdio`** (`core/src/mcp_client.rs`) que lanza los
>   servidores habilitados, hace el handshake `initialize` + `tools/list` y
>   ejecuta `tools/call`.
> - Inyección automática de las tools MCP descubiertas en el bucle del agente.
>   Cada tool aparece como `mcp__<servidor>__<tool>` y el modelo puede elegirla
>   junto con las tools locales.
> - Un **adaptador MCP delgado** (`core/src/mcp_adapter.rs`) que traduce llamadas
>   tipo MCP a envelopes del protocolo LAN de MultiLink y viceversa.
>
> Al guardar la configuración desde la GUI, MultiLink refresca el catálogo sin
> reiniciar la aplicación. El primer mensaje tras arrancar descubre las tools;
> después se reutiliza el catálogo en caché.

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

### 6.3 Ejecución de servidores MCP externos

`core/src/mcp_client.rs` implementa el cliente MCP local:

1. Lee `[mcp].servers` al iniciar (o al guardar desde la GUI).
2. Lanza cada servidor habilitado por `stdio` con `command`, `args` y `env`.
3. Hace el handshake `initialize` / `notifications/initialized`.
4. Descubre tools con `tools/list` (incluyendo paginación por cursor).
5. Expone cada tool al selector del agente como `mcp__<servidor>__<tool>`.
6. Ejecuta `tools/call` cuando el modelo elige una de esas tools.
7. Cierra el proceso al terminar cada descubrimiento o llamada.

Consideraciones prácticas:

- El catálogo se descubre una vez y se reutiliza. Si cambias servidores en la
  GUI, se limpia y se vuelve a descubrir en el siguiente mensaje.
- Las tools MCP tienen prioridad sobre la heurística de "chat personal": aunque
  la sesión no tenga carpeta de proyecto, el agente puede usar MCP.
- Los errores de arranque o de protocolo se registran en stderr con el prefijo
  `[mcp]`. Si un servidor falla, el resto sigue disponible.
- Los procesos se lanzan por operación (descubrimiento o llamada), no se
  mantiene un daemon persistente entre mensajes.

### 6.4 Limitaciones conocidas

- **Modelos locales pequeños**: pueden omitir campos obligatorios del schema o
  devolver JSON incompleto. El selector refuerza las reglas y ya no ejecuta
  tools MCP con `args={}` cuando hay campos requeridos, pero puede necesitar
  varios pasos para completar la tarea.
- **Fallback heurístico**: si el modelo no produce JSON válido y la tool MCP
  requiere argumentos, el agente se detiene y pide los datos en lugar de
  inventarlos. Solo se eligen automáticamente tools MCP sin argumentos
  obligatorios.
- **Cantidad de operaciones**: para "crea N elementos" se cuentan las llamadas
  `create` exitosas y se fuerza a continuar hasta N. Es una heurística: no
  entiende objetivos más complejos ni cantidades expresadas de forma ambigua.
- **Concurrencia**: enviar un segundo mensaje mientras el primero sigue en el
  bucle de tools puede mezclar el contexto visible y las respuestas finales.
  Envía un mensaje y espera a que termine.
- **Descubrimiento**: el catálogo se descubre en el primer mensaje. Guardar la
  configuración limpia la caché, pero no hay reconexión en caliente de procesos
  ya lanzados.
- **Recuperación de configuración**: si `multilink.toml` es inválido, el
  arranque restaura los valores por defecto y puede perder `[mcp]`. Conserva el
  respaldo `multilink.invalid-*.toml` y vuelve a añadir los servidores.
- **`think=false`**: depende de una versión de Ollama que soporte el campo
  `think`; en versiones antiguas se ignora y el modelo puede tardar más en el
  selector.

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

# A8b: servir CodeLoom como agente A2A

Estado: propuesta del 2026-10-23. La persona usuaria ordenó abordarla («haz la 1, 2 y 3»); las
decisiones de seguridad de abajo son recomendaciones nuestras y quedan rotuladas para su revisión.
Continúa `2026-10-23-codeloom-a2a-remote-transport-design.md` (D1, D7, D9) y `…-remote-auth-design.md`.

## Premisa

Cabe señalar que servir es lo contrario de consumir: ahora es un **tercero** quien dicta a un modelo
de CodeLoom lo que ha de hacer, sin una persona delante que apruebe permisos. Por eso esta spec trata
la seguridad como el diseño y no como un añadido: qué puede hacer el agente servido, quién puede
llamarlo y desde dónde. A2A «no prescribe» el modelo de autorización (§13.1); todo lo que sigue y no
cite el protocolo es decisión nuestra.

## Lo que consta (de la lectura literal de la especificación, ya anotada)

- Tarjeta pública en `/.well-known/agent-card.json` (§8.1–8.2); la primera interfaz es la preferida (§8.3).
- Métodos de JSON-RPC en PascalCase: `SendMessage`, `GetTask`, `CancelTask`… (§9.1).
- Errores: `TaskNotFoundError` −32001, `TaskNotCancelableError` −32002,
  `PushNotificationNotSupportedError` −32003, `UnsupportedOperationError` −32004,
  `ContentTypeNotSupportedError` −32005, `VersionNotSupportedError` −32009 (§5.4).
- `A2A-Version` obligatoria en cada petición; vacía equivale a `0.3`; sin degradación (§3.6).
- Seguridad (§13): autorización en cada operación y antes de consultar datos; **no revelar la
  existencia** de recursos ajenos (§3.3.2); push sin soportar ⇒ `PushNotificationNotSupportedError`;
  tarjeta extendida sin declarar ⇒ `UnsupportedOperationError` (§3.3.4); HTTPS en producción (§13.4).
- «Los ids los genera el servidor» (tareas) y una tarea terminal no se reinicia (§3.1).
- `SendMessage` bloquea por defecto hasta estado terminal o interrumpido; `returnImmediately`
  lo evita (§3.2.2).

## Lo que no consta (y por tanto es nuestro)

- Qué puede ejecutar un agente servido, cuántos a la vez y durante cuánto: A2A no lo dice.
- Qué hacer cuando el tope de espera de un envío bloqueante vence antes de un estado terminal.
- Código de error para «servidor ocupado»: no hay uno en la tabla de §5.4.
- Cómo se enlazan las credenciales con los recursos (modelo de acceso, §13.1).

## Decisiones (nuestras salvo indicación)

- **D1. Alcance:** HTTP + JSON-RPC, sin flujo. Métodos soportados: `SendMessage`, `GetTask`,
  `CancelTask`. `SendStreamingMessage` y `SubscribeToTask` (A8c), `ListTasks` y las operaciones de
  tarjeta extendida responden `UnsupportedOperationError`; las de notificaciones push,
  `PushNotificationNotSupportedError`; un método desconocido, `MethodNotFound` (−32601).
- **D2. Sin autenticación no hay servidor.** Hay que configurar un esquema (`bearer` o `apiKey` en
  cabecera) y la variable de entorno que guarda el secreto; si falta, está vacía o el esquema es otro,
  el rol `serve` **no arranca** y lo dice. La comparación es en tiempo constante, el secreto no se
  registra ni sale en ningún error, y toda petición a `/a2a` sin credencial correcta recibe 401 con
  `WWW-Authenticate` y sin cuerpo informativo. La tarjeta es la única ruta pública.
- **D3. Un solo agente servido:** `serve.agent`, por defecto `default`, tomado del registro. Los remotos
  no se sirven (no se reexpone a un tercero).
- **D4. Herramientas de solo lectura.** El agente servido dispone de `view`, `glob` y `grep`, y de nada
  más: ni `edit`, ni `bash`, ni jobs, ni subagentes, ni `ask_user`, ni `todo`. Motivo: nadie aprobaría
  un permiso, y la regla del proyecto es no delegar escritura en quien no está presente. Ampliarlo
  exigiría su propia spec (lista blanca por herramienta y por ruta).
- **D5. Dónde escucha:** solo en bucle local (`127.0.0.1` o `::1`), puerto configurable. Una dirección
  no local es un error de configuración: el proceso no habla TLS (§13.4 exige cifrado en
  producción), así que quien quiera exponerlo ha de ponerle delante un proxy TLS que sí lo haga y
  apuntar `public_url` a él.
- **D6. Un solo principal.** Hay un único secreto compartido, luego un único «propietario»: todas las
  tareas pertenecen a esa credencial. Las tareas se crean bajo una sesión padre propia del servidor y
  solo se consultan o cancelan las de esa sesión; cualquier otro id (inexistente o ajeno) da el mismo
  `TaskNotFoundError`, sin distinguir. Multiusuario queda fuera.
- **D7. Envío bloqueante acotado.** Sin `returnImmediately`, `SendMessage` espera a un estado terminal
  hasta `serve.max_wait_secs` (30 por defecto). Si vence, devuelve la tarea en su estado actual, que
  no es lo que dice §3.2.2 (terminal o interrumpido); se rotula como desviación nuestra y el cliente
  la sigue con `GetTask`.
- **D8. Todo con cota (decálogo):** cuerpo de petición 64 KiB; texto del mensaje 32 KiB; hasta 4 tareas
  vivas a la vez (la quinta recibe HTTP 503 con un error JSON-RPC −32000 «servidor ocupado», código
  nuestro); una tarea que lleve más de `serve.max_task_secs` (10 minutos) se cancela; el texto de los
  artefactos que se devuelve se acota a `max_result_bytes`.
- **D9. Contenido:** solo partes de texto. `raw`, `url` y `data` dan `ContentTypeNotSupportedError`.
  Un mensaje con `taskId` (continuación de una tarea existente) da `UnsupportedOperationError`: el
  agente servido nunca pide entrada (D4), así que no hay nada que reanudar.
- **D10. `A2A-Version`:** solo `1.0`. Ausente, vacía o distinta ⇒ `VersionNotSupportedError`, sin
  degradar a `0.3`.
- **D11. Tarjeta servida:** nombre y descripción del agente servido (acotados), interfaz JSON-RPC `1.0`
  en `public_url` (o en la dirección de escucha), `capabilities` con `streaming`, `pushNotifications` y
  `extendedAgentCard` a `false`, `securitySchemes` y `securityRequirements` con el esquema configurado,
  `skills` vacía (A6 no las sintetiza todavía). Lleva `ETag` de su contenido y
  `Cache-Control: max-age=60` (SHOULD de §8.6).
- **D12. Visibilidad:** cada tarea entrante genera un aviso en la conversación («tarea A2A entrante
  `<id>` → `<agente>`: working/completed…») para que la persona sepa que su proceso está trabajando
  para un tercero. Sin esto, servir sería invisible, y la observabilidad es el valor central.
- **D13. Pruebas:** el enrutador se prueba en memoria, sin puertos. Dos excepciones declaradas: un
  recorrido de nuestro cliente (A8a) contra nuestro servidor por bucle local, y una prueba viva a mano
  con un cliente independiente (el del SDK oficial).

## Configuración

```toml
[a2a]
role = "serve"            # o "both"

[a2a.serve]
listen = "127.0.0.1:7077" # solo bucle local
agent = "default"
auth = { scheme = "bearer", env = "CODELOOM_A2A_TOKEN" }
# o: auth = { scheme = "apikey", name = "X-Api-Key", env = "CODELOOM_A2A_KEY" }
# public_url = "https://loom.ejemplo.org/a2a"   # si hay un proxy TLS delante
# max_concurrent_tasks = 4
# max_wait_secs = 30
# max_task_secs = 600
```

## Fuera de alcance, declarado

- Flujo SSE y reanudación (A8c); notificaciones push; tarjeta extendida; gRPC y REST.
- Varios principales o varios agentes servidos; permisos por herramienta o por ruta.
- TLS dentro del proceso; limitación de ritmo por cliente (en bucle local no hay cliente anónimo
  remoto, pero un proxy que lo exponga debería aplicarla).
- Servir agentes con escritura.

## Preguntas abiertas (resueltas con la opción recomendada, a revisar por la persona usuaria)

1. ¿Agente servido de solo lectura (D4)? Recomendado: sí; es el único modo seguro sin persona delante.
2. ¿Solo bucle local (D5) y TLS por proxy? Recomendado: sí.
3. ¿Un único secreto compartido (D6)? Recomendado: sí por ahora; el multiusuario necesita su propia spec.

## Estado

Sin implementar.

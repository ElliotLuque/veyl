# Comprobaciones

Desde la raíz:

```sh
sh tests/check.sh
cargo build --release --locked --manifest-path veyld/Cargo.toml
```

Requiere Rust/Cargo con rustfmt y Clippy (Rust 1.89 o posterior por las APIs que ya usa el proyecto), Python 3 y Node.js 18 o posterior. Las pruebas de procesos requieren Linux y las utilidades `sh`, `cat`, `sleep` y `yes`. No necesitan Hyprland ni un portapapeles real y no modifican el estado del usuario.

## Cobertura de regresión

- **Rust:** no registrar actividad cuando el diario está desactivado; ocultar títulos y carpetas de ventanas privadas; permisos de SQLite, WAL y SHM; recorte retroactivo de inactividad; intervalos breves; cambios de metadatos; pausa del diario; duraciones fuera de rango; expresiones temporales contradictorias; conservación de argumentos entre comillas; ciclos de enlaces simbólicos en aplicaciones; marcas temporales con zona horaria; mensajes IPC limitados; invalidación de confirmaciones antiguas; tiempo y tamaño máximos de subprocesos, incluidos sus descendientes; notificaciones: caducidad y pausa de los avisos, no molestar, sustitución por id, retirada, pasajeras, acciones (residentes o no), limpieza por antigüedad, persistencia sin vida tras reiniciar con permisos privados, lectura de pistas de D-Bus e imágenes en datos convertidas a PNG.
- **Python:** JSON limpio al listar por primera vez; PNG truncados; entradas secretas; lectura acotada de datos grandes; permisos privados; deduplicación y fijación; límite del historial; rechazo de índices corruptos sin sobrescribirlos; copia concurrente con borrado.
- **JavaScript:** ejecuta las funciones reales extraídas de los archivos QML para comprobar fechas y horas, identificadores, datos malformados, restauración y persistencia de la agenda, historial fijado, resultados obsoletos de la calculadora y temporizadores inválidos.

## Límites

Las pruebas JavaScript no ejecutan el motor de bindings de Qt, PAM, PipeWire, D-Bus, ni el compositor. `qmllint` necesita los módulos de la versión de Quickshell utilizada y resolver el módulo virtual `qs`; sus avisos de importación no equivalen por sí solos a fallos en ejecución. La validación completa de bloqueo, notificaciones, reconexión y salida de sesión debe hacerse en una sesión de Hyprland de prueba.

Los cambios de privacidad evitan **nuevas** grabaciones indebidas: no borran ni migran títulos que pudiera haber guardado una versión anterior del diario. El daemon relee los ajustes cada 15 segundos. Para usar un binario recién compilado hay que reiniciar `veyld`; recargar solo QML no sustituye el daemon que ya está ejecutándose.

# Журнал LipaX

LipaX подключает `tracing` / `tracing-subscriber` до создания Qt и фоновых задач.
Журнал доступен при запуске из Bash, Zsh, Konsole и других терминалов. Отдельное
окно журнала и файл по умолчанию не создаются.

- `ERROR` → stderr.
- `WARN`, `INFO`, `DEBUG`, `TRACE` → stdout.
- По умолчанию включён `INFO` и более серьёзные сообщения.
- Записи содержат время, уровень, Rust-модуль или категорию Qt; при наличии —
  компонент, исходный файл и строку.
- При перенаправлении потоков управляющие ANSI-последовательности отключены.

После установки пакета доступны команды `lipax` и прежняя `lipa`.

```bash
RUST_LOG=debug lipax
RUST_LOG=trace lipax
RUST_LOG=warn,lipa=debug,lipa_core=trace,qt=debug lipax
```

Только ошибки и предупреждения:

```bash
RUST_LOG=warn lipax
```

Сохранить обычные сообщения и ошибки отдельно:

```bash
RUST_LOG=debug lipax > lipax.log 2> lipax-errors.log
```

Показать и одновременно сохранить оба потока:

```bash
RUST_LOG=debug lipax 2>&1 | tee lipax.log
```

Некорректный `RUST_LOG` вызывает предупреждение и возврат к `INFO`.

## Qt / QML

`qInstallMessageHandler` направляет сообщения Qt в тот же backend:

| Qt | Уровень журнала |
|---|---|
| qDebug, console.debug / console.log | DEBUG |
| qInfo, console.info | INFO |
| qWarning, console.warn, ошибки привязок QML | WARN |
| qCritical, console.error | ERROR |
| qFatal | ERROR с категорией `qt::fatal`, затем штатное аварийное завершение Qt |

Сообщения сохраняют категорию Qt, файл и строку, если Qt их предоставляет.
Собственные фильтры категорий Qt (`QT_LOGGING_RULES`) продолжают действовать.
Чтобы включить также категории отладки, отключённые самой Qt:

```bash
QT_LOGGING_RULES='*.debug=true' RUST_LOG=debug lipax
```

## Ошибки ядра и аварийное завершение

Ошибки pipeline записываются **до** передачи события в Qt: закрытый канал или
недоступное окно не теряют причину. Отдельно журналируются сохранение/чтение
настроек и истории, зависимости, KGlobalAccel/D-Bus, выбор окна и создание подложки.

`DEBUG` показывает этапы OCR/перевода, размеры кадров, языки, HTTP-статусы, вызовы
Portal и диагностику процессов Tesseract, PaddleOCR и GStreamer. `TRACE` добавляет
получение кадров KWin. Код приложения не печатает конфигурацию целиком, OCR-текст,
переводы или токены Portal; URL удаляется из сетевых ошибок перевода, поскольку
в query может находиться исходный текст.

Rust panic печатает причину, место, поток и принудительно захваченный backtrace
в stderr. `RUST_BACKTRACE=1` не требуется. Panic и qFatal сохраняются даже при
`RUST_LOG=off`; фильтры обычных сообщений на них не распространяются. В stripped
release-сборке часть кадров backtrace может быть без имён функций.

## Проверка

`cargo test --workspace` включает subprocess-проверки настоящих stdout/stderr,
фильтров, qDebug/qWarning/qCritical/qFatal, QML console и ошибок привязок,
а также panic без переменной `RUST_BACKTRACE`.

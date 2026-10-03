# LipaX

### Русский

### Это чистый ВАЙБ КОД !!!! ,зарание извините.

Легковесный инструмент для перевода текста с экрана под Wayland, написанный на Rust и Qt 6 / QML.

#### Установка (Arch Linux)

Клонируйте репозиторий и соберите пакет:

```
git clone https://github.com/GrayRats/lipax-qt.git
cd lipax-qt
makepkg -si

```

#### Использование

1. Запустите приложение из терминала или через меню вашего окружения:

   ```
   lipa
   
   ```

2. Выделите нужную область экрана с текстом, который хотите перевести.

3. Просмотрите результат перевода в появившемся интерфейсе.

### English

### This is VIBE CODING!!
 
A lightweight Wayland screen translation tool built with Rust and Qt 6 / QML.

#### Installation (Arch Linux)

Clone the repository and build the package:

```
git clone https://github.com/GrayRats/lipax-qt.git
cd lipax-qt
makepkg -si

```

#### Usage

1. Launch the application from your terminal or desktop environment:

   ```
   lipa
   
   ```

2. Select the desired screen area containing the text you want to translate.

3. View the translation result in the pop-up interface.


### Сборка текущих исходников (Arch Linux)

```bash
./packaging/build-local.sh
```

Пакет появится в `dist/`. Установка от root:

```bash
pacman -U /полный/путь/к/LipaXQT-0.3.0-6-x86_64.pkg.tar.zst
```

[Список доработок и предложения](docs/IMPROVEMENTS.md) ·
[Подключение PaddleOCR](docs/PaddleOCR.md)

### Диагностика в терминале

```bash
RUST_LOG=debug lipax
# сохранить оба потока:
RUST_LOG=debug lipax 2>&1 | tee lipax.log
```

ERROR выводится в stderr, остальные уровни — в stdout. По умолчанию — INFO.
[Подробности журналирования Rust и Qt/QML](docs/LOGGING.md).

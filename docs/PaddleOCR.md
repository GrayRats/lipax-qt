# PaddleOCR (необязательный движок)

LipaX использует Python API PaddleOCR **3.x** (`PaddleOCR.predict`, `rec_texts`).
Tesseract остаётся движком по умолчанию. PaddleOCR работает на CPU в отдельном
долгоживущем процессе: модель загружается один раз на выбранный язык.
Кадры передаются локально через stdin, временные скриншоты не создаются.

Создайте отдельное окружение от обычного пользователя (не root):

```bash
python3 -m venv ~/.local/share/lipa/paddle-venv
~/.local/share/lipa/paddle-venv/bin/python -m pip install 'paddlepaddle>=3,<4' 'paddleocr>=3,<4'
```

Для Python вашей системы должен существовать совместимый wheel PaddlePaddle.
Если pip сообщает `No matching distribution`, используйте поддерживаемую
PaddlePaddle версию Python для создания venv.

В «Настройки → Распознавание» выберите **PaddleOCR 3.x**, основной язык,
и укажите **абсолютный путь** к `~/.local/share/lipa/paddle-venv/bin/python`
(например, `/home/art/.local/share/lipa/paddle-venv/bin/python`). Настройка сохраняется автоматически.
Дополнительные языки через `+` применяются только в Tesseract.

Первый запуск может скачать модели из сети; следующие используют локальный кэш.
Общий лимит операции в приложении — 60 секунд. Если первоначальная загрузка
моделей не успевает, подготовьте их отдельно (пример ниже), затем нажмите «Повторить».
После ошибки worker перезапускается при следующем запросе. Ошибка импорта, отсутствие модели или неподдерживаемый язык
отображаются в статусе приложения.

Подготовка моделей для выбранного языка до запуска приложения (например, английского):

```bash
~/.local/share/lipa/paddle-venv/bin/python - <<'PYTHON'
from paddleocr import PaddleOCR
PaddleOCR(lang="en", use_doc_orientation_classify=False,
          use_doc_unwarping=False, use_textline_orientation=False)
PYTHON
```

Интеграция поддерживает: eng, rus, jpn, kor, chi_sim, chi_tra, deu, fra, spa, ita,
por, ukr, pol. Качество стилизованных шрифтов зависит от игры: сравните оба движка
на одной области.

Документация API и установки:
https://www.paddleocr.ai/v3.0.0/en/version3.x/pipeline_usage/OCR.html
https://www.paddlepaddle.org.cn/install/quick

<p align="center"><img src="docs/images/mascot.png" width="200" height="200" alt="Сова BOTUI на двух книгах"></p>

# botui

[English](README.md) · **Русский**

BOTUI — терминальная читалка для Linux. Откройте книгу с диска, найдите издание в каталоге OPDS или подключите свой аккаунт BookOrbit.

[Скриншоты](#скриншоты) · [Установка](#установка) · [Первый запуск](#первый-запуск) · [Руководство](docs/guide.ru.md)

## Библиотеки и чтение

- **Локальные папки.** Листайте обложки или список, ищите книги по мере ввода и возвращайтесь к месту, где остановились.
- **BookOrbit (https://github.com/bookorbit/bookorbit).** Выбирайте книги по авторам, сериям и коллекциям. При первом открытии издание сохраняется на устройстве. Читайте офлайн — позиция синхронизируется при подключении к аккаунту.
- **OPDS.** Просматривайте каталоги OPDS 1 и 2. Скачанные издания доступны на отдельной полке.
- **Читалка.** EPUB и FB2 подстраиваются под ширину окна, PDF и DjVu сохраняют исходную вёрстку. Доступны одна страница, разворот и прокрутка. Можно переходить по сноскам, ставить закладки и экспортировать заметки в Markdown.
- **Технические EPUB.** Таблицы с объединёнными ячейками, формулы и примеры кода видны прямо в тексте. После изменения ширины окна закладки и позиция BookOrbit по-прежнему ведут к нужному месту.

## Скриншоты

Нажмите на изображение, чтобы открыть его в полном размере.

<table>
  <tr>
    <td width="50%" valign="top">
      <a href="docs/images/start.png"><img src="docs/images/start.png" alt="Стартовый экран"></a><br>
      <strong>Стартовый экран</strong><br><sub>Catppuccin Mocha · 16 pt</sub>
    </td>
    <td width="50%" valign="top">
      <a href="docs/images/dashboard.png"><img src="docs/images/dashboard.png" alt="Дашборд BookOrbit"></a><br>
      <strong>Дашборд BookOrbit</strong><br><sub>Gruvbox Dark · 12 pt</sub>
    </td>
  </tr>
  <tr>
    <td width="50%" valign="top">
      <a href="docs/images/bookorbit-search.png"><img src="docs/images/bookorbit-search.png" alt="Поиск по мере ввода в BookOrbit"></a><br>
      <strong>Поиск по мере ввода в BookOrbit</strong><br><sub>Tokyo Night · 13 pt</sub>
    </td>
    <td width="50%" valign="top">
      <a href="docs/images/reading-spread.png"><img src="docs/images/reading-spread.png" alt="Разворот EPUB"></a><br>
      <strong>Разворот EPUB</strong><br><sub>Sepia Dark · 12 pt</sub>
    </td>
  </tr>
</table>

### Другие темы и размеры шрифта

<table>
  <tr>
    <td width="50%" valign="top">
      <a href="docs/images/shelves-overview-ru.jpg"><img src="docs/images/shelves-overview-ru.jpg" alt="Стартовый экран и библиотеки"></a><br>
      <strong>Стартовый экран и библиотеки</strong>
    </td>
    <td width="50%" valign="top">
      <a href="docs/images/reading-overview-ru.jpg"><img src="docs/images/reading-overview-ru.jpg" alt="Режимы чтения"></a><br>
      <strong>Режимы чтения</strong>
    </td>
  </tr>
</table>

## Установка

Выполняйте команды из корня репозитория. Каждый из трёх способов сохраняет исполняемый файл для Linux в `./dist/botui`.

### Через mise

```sh
mise trust
mise install rust
mise run install
```

Задача install собирает релиз для вашей машины. Чтобы получить статические бинарники сразу для **x86_64 и aarch64**, выполните `mise install` и `mise run dist`. Оба файла и `SHA256SUMS` появятся в `./dist/`.

Cargo по умолчанию хранит в `target/` отдельный кэш для родной сборки и каждой из двух статических. При первой сборке каждого варианта компилируются его зависимости. Повторный запуск той же задачи использует готовые файлы.

### Через Cargo

Версия Rust указана в `rust-toolchain.toml`. Для сборки нужны rustup, компилятор C и компоновщик.

```sh
cargo build --release --locked -p botui --target-dir target &&
  install -Dm755 target/release/botui ./dist/botui
```

### Готовые бинарники для Linux

Скачайте `botui-x86_64` или `botui-aarch64` для вашей архитектуры и файл `SHA256SUMS` из [релизов](https://github.com/gAlleb/botui/releases) или [сборок ветки](https://github.com/gAlleb/botui/actions/workflows/build.yml).

### Поддержка форматов

| Формат | Что нужно для чтения |
| --- | --- |
| EPUB, FB2 | Поддержка встроена |
| PDF | Утилиты Poppler (`pdfinfo`, `pdftoppm`, `pdftotext`) или Python 3 с Poppler GI и Cairo |
| DjVu | DjVuLibre (`djvused`, `ddjvu`, `djvutxt`) |

Kitty показывает обложки и страницы как изображения. В других терминалах доступен вывод цветными полублоками. Для поиска внутри PDF и DjVu нужен текстовый слой.

## Первый запуск

```sh
botui                          # Выбрать библиотеку
botui --library "$HOME/Books"  # Запомнить локальную папку
botui ./examples/first-light.fb2
botui --resume                 # Продолжить последнюю недочитанную локальную книгу
```

На стартовом экране **Enter** открывает локальную библиотеку, **B** — BookOrbit, **P** — OPDS. Для поиска на полке нажмите **/** и начните вводить название. Выберите книгу стрелками и нажмите **Enter** или щёлкните по ней.

| Во время чтения | Клавиша |
| --- | --- |
| Одна страница, разворот, непрерывная прокрутка | **1**, **2**, **3** |
| Переключиться между дзеном и видом с оглавлением | **Z** |
| Показать или скрыть страницу и процент прочитанного в дзен-режиме | **%** |
| Настроить ширину текста и зазор или масштаб скана | **W** |
| Найти текст в книге | **/** |
| Перейти на страницу PDF или DjVu по номеру | **P** |
| Вернуться на полку | **Q** |

## Настройки и справка

**F2** открывает настройки, **?** — подсказки для текущего экрана. При первом запуске включена тема **Catppuccin Mocha**. Выберите **Terminal**, чтобы использовать палитру терминала.

Команда `botui --config-path` покажет путь к TOML-файлу. В [примере конфига](config.example.toml) есть папка библиотеки, темы, клавиши и вход в BookOrbit по паролю или через команду менеджера паролей.

[Как создавать и удалять коллекции, добавлять и убирать книги](docs/guide.ru.md#управление-коллекциями)

В [руководстве](docs/guide.ru.md) описаны синхронизация после чтения офлайн, закладки, заметки, сноски и управление PDF.

## Разработка

| Команда | Что проверяет |
| --- | --- |
| `mise run check` | Форматирование, Clippy и тесты Rust |
| `mise run smoke` | Работу библиотеки и читалки в изолированных терминальных сеансах |

Для smoke-тестов нужны tmux, Python 3.11+, указанные выше средства чтения PDF и DjVu, а также `c44` и `djvm` для создания тестовых файлов.

[Архитектура](docs/architecture.ru.md) · [Проверяемые сценарии](docs/checklist.ru.md) · [Границы проверок](docs/checklist.ru.md#границы-проверок) · [История изменений (EN)](CHANGELOG.md)

## Лицензия

BOTUI и собственные обложки-заглушки распространяются под [GPL-3.0-or-later](LICENSE). Права на обложки из книг и каталогов сохраняются за их правообладателями.

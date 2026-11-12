<p align="center">
  <img src="docs/images/mascot.png" width="30%" alt="Сова BOTUI на двух книгах">
  <img src="docs/images/wordmark.svg" width="60%" alt="BOTUI">
</p>

# botui

[English](README.md) · **Русский**

BOTUI — терминальная читалка для Linux. Читайте книги, документы и комиксы из локальных папок, своей [библиотеки BookOrbit](https://github.com/bookorbit/bookorbit) или каталогов OPDS. Поддерживаются EPUB, FB2, Markdown, PDF, DjVu, CBZ и CBR.

[Возможности](#возможности) · [Скриншоты](#скриншоты) · [Установка](#установка) · [Первый запуск](#первый-запуск) · [Форматы](#форматы) · [Чтение](#чтение) · [Управление](#управление) · [Настройки](#настройки) · [Руководство](docs/guide.ru.md)

## Возможности

- Сетка обложек, коллекции и поиск по библиотеке по мере ввода
- Офлайн-загрузки BookOrbit и синхронизация позиции чтения между устройствами
- Одна страница, разворот и непрерывная прокрутка с настройкой полей, зазоров и масштаба
- Независимые темы интерфейса и чтения, предпросмотр и дзен-режим
- Выделение текста, цветные хайлайты, редактируемые комментарии, закладки и экспорт в Markdown
- Значения слов и перевод через настраиваемые HTTP-подключения, OpenAI-совместимые API или локальные программы
- Управление мышью, меню команд Space и необязательный курсор Vim
- Таблицы, формулы и иллюстрации EPUB; оформленный Markdown с подсветкой кода и просмотром исходника

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

### Готовые бинарники для Linux

Скачайте исполняемый файл для вашей архитектуры (`botui-x86_64` или `botui-aarch64`). В каждом релизе есть файл контрольных сумм `SHA256SUMS`. [Релизы для Linux](https://github.com/gAlleb/botui/releases)

В сборках веток те же файлы упакованы в `botui-linux.tar.gz`. [GitHub Actions](https://github.com/gAlleb/botui/actions/workflows/build.yml)

Чтобы установить скачанный бинарник для своего пользователя (на ARM64 замените имя на `botui-aarch64`):

```sh
install -Dm755 ./botui-x86_64 "$HOME/.local/bin/botui"
```

Каталог `~/.local/bin` должен быть в `PATH`. Также можно запускать файл по полному пути.

### Через mise

Для сборки из исходников установите компилятор C, компоновщик и Python 3. Движок вёрстки EPUB использует `python3` во время компиляции. Rust и CMake установит mise. Выполните команды из корня репозитория.

```sh
mise trust
mise install rust cmake
mise run install
```

Задача install собирает версию для вашей машины и кладёт её в `./dist/botui`. Чтобы получить статические бинарники сразу для **x86_64 и aarch64**, выполните `mise install` и `mise run dist`. Оба файла и `SHA256SUMS` появятся в `./dist/`.

Cargo по умолчанию хранит в `target/` отдельный кэш для родной сборки и каждой из двух статических. При первой сборке каждого варианта компилируются его зависимости. Повторный запуск той же задачи использует готовые файлы.

### Через Cargo

Установите rustup, компилятор C, компоновщик, CMake и Python 3. При запуске из корня репозитория Cargo использует версию Rust из `rust-toolchain.toml`.

```sh
cargo build --release --locked -p botui --target-dir target &&
  install -Dm755 target/release/botui ./dist/botui
```

## Первый запуск

```sh
botui                          # Выбрать библиотеку
botui --library "$HOME/Books"  # Запомнить локальную папку
botui ./examples/first-light.fb2
botui --resume                 # Продолжить последнюю незавершённую локальную книгу или загрузку
```

Нажмите **Enter/L** для локальной библиотеки, **B** для BookOrbit или **P** для OPDS. Выбирайте стрелками и **Enter** либо щелчком мыши. **F1** показывает управление текущим экраном, **F2** открывает настройки. Названия пунктов интерфейса — на английском.

## Библиотеки

Для всех источников используются одни и те же ридеры поддерживаемых форматов. BookOrbit и OPDS должны предоставлять доступное для скачивания издание в одном из этих форматов.

| Источник | Как пользоваться |
| --- | --- |
| Локальные папки | Просматривайте обложки или имена файлов, ищите через **/** и собирайте коллекции. **P** выбирает папку, **R** делает текущую папку корнем библиотеки. |
| BookOrbit | Войдите в аккаунт, выберите книгу по названию, автору, серии или коллекции и откройте издание из карточки. **Read** сохраняет скачанную копию; **D Save offline** загружает её заранее. Позиция чтения синхронизируется с аккаунтом. |
| OPDS | Откройте каталог OPDS 1 или 2, найдите книгу и выберите доступное для скачивания издание. Сохранённые каталоги настраиваются в **F2 → Library → OPDS catalogs**. Позиция чтения хранится на этом устройстве. |

На полке **V** переключает обложки/список, **/** открывает поиск. **Q** возвращает на шаг назад: из ридера BookOrbit — к карточке и исходной полке, из локальной книги — к папке и её родительским папкам. Выделение и прокрутка сохраняются. На экране выбора источника **Q** завершает BOTUI; закрытие файла, открытого напрямую из командной строки, завершает эту команду.

Скачанные копии доступны через **D BookOrbit offline** и **O OPDS downloads** на стартовом экране. [Офлайн-синхронизация](docs/guide.ru.md#офлайн-чтение-и-синхронизация) · [Управление коллекциями](docs/guide.ru.md#управление-коллекциями)

## Форматы

| Формат | Возможности чтения | Что требуется для запуска |
| --- | --- | --- |
| EPUB / FB2 | Перевёрстка текста, оглавление, иллюстрации, поиск и пометки к фрагментам. EPUB также отображает оформленные таблицы, MathML/размеченный TeX и статические SVG. | Поддержка встроена |
| Markdown (`.md`, `.markdown`) | Заголовки, списки, цитаты, задачи, таблицы, подсветка кода и локальные картинки. **F9** переключает оформление/исходник; выделение и заметки работают в обоих видах. | Поддержка встроена |
| PDF | Фиксированные страницы, оглавление, масштаб, поворот и заметки на страницу. Текстовый слой позволяет искать; `pdftotext` также даёт выделение слов и хайлайты. | Утилиты Poppler (`pdfinfo`, `pdftoppm`, `pdftotext`) либо Python 3 с Poppler GI и Cairo для отрисовки |
| DjVu | Фиксированные страницы, оглавление, масштаб, поворот и заметки на страницу; поиск при наличии текстового слоя. | DjVuLibre (`djvused`, `ddjvu`, `djvutxt`) |
| CBZ / CBR | Картинки в естественном порядке имён, оглавление, масштаб, поворот, закладки и заметки на страницу. | Встроенные декодеры ZIP/RAR; внешний распаковщик не нужен |

Для оглавления PDF нужны Python 3, PyGObject и привязки Poppler GI. EPUB, состоящие из изображений, открываются в ридере фиксированных страниц. OCR нет: сканы и картинки комиксов не дают выделения слов, словаря и перевода.

В оформленном Markdown уровни заголовков отмечены номерами и цветом, списки сохраняют номера и вложенность, а код, цитаты и шапки таблиц выделены фоном темы чтения. Блоки NOTE/TIP/IMPORTANT/WARNING/CAUTION оформлены отдельно. **F9** открывает исходник с подсветкой Markdown.

Markdown читает локальные изображения из каталога документа и его подкаталогов; удалённые изображения не скачиваются. HTML показывается текстом, MDX и плагины Obsidian не исполняются. Заметки сохраняют позиции при переключении вида и изменении ширины; внешняя правка исходника может сдвинуть их. Лимиты: 4 МиБ текста Markdown и 16 МиБ на картинку.

Страницы комиксов могут быть JPEG, PNG или WebP, до 16 МиБ каждая. Запароленные и многотомные RAR не поддерживаются. Для файлов `.cbz` и `.cbr` тип архива ZIP или RAR определяется по содержимому: комиксы с перепутанным расширением тоже открываются, переименовывать их не нужно.

Kitty показывает обложки и страницы картинками, в том числе внутри tmux с пробросом графики. Терминалы без графического протокола могут использовать цветные полублоки. [Подробные ограничения форматов](docs/checklist.ru.md#границы-проверок)

## Чтение

### Страницы и размер текста

**1 / 2 / 3** выбирают одну страницу, разворот или непрерывную прокрутку. В Vim эти цифры задают счётчик; для смены режима используйте **Space l 1 / 2 / 3**. **Space o** открывает оглавление, **Space z** переключает дзен-режим, **Space p** — строку прогресса в нём.

**Space l w** настраивает ширину текста и расстояние между колонками. Для PDF, DjVu, комиксов и EPUB из изображений это же окно управляет масштабом и зазором разворота. Кнопки зазора или **[ / ]** задают **0–32 ячейки терминала**; ноль соединяет страницы. `scan_gap` сохраняет это значение отдельно от текстовых колонок. **+ / −** меняют масштаб фиксированных страниц, стрелки перемещают их, **Space r 0** возвращает Fit page (Fit width в прокрутке). Увеличенные страницы занимают свободные поля и обрезаются только у края области чтения.

Размер и гарнитуру текста задаёт терминал: увеличивайте или уменьшайте его шрифт, и BOTUI переверстает страницу. В **F2 → Reader** настраиваются красная строка, интервалы между абзацами и шаг колеса мыши. Чтобы изменить ширину оглавления, перетащите разделитель.

В Foot/Sixel быстрые жесты колёсика и тачпада показывают промежуточные кадры с ограниченным шагом, сохраняя заданную дистанцию прокрутки. Открытие меню или навигационная клавиша останавливает накопленное движение. Передача больших изображений всё ещё занимает больше времени, чем в Kitty.

### Темы

**F2 → Appearance → Interface theme** задаёт оформление полок, настроек и остальных экранов интерфейса. **Space t** меняет тему чтения поверх открытой страницы: ↑/↓ показывают палитру, ←/→ выбирают светлый/тёмный вариант, Enter или щелчок применяет, Escape отменяет. Начальная тема интерфейса — Catppuccin Mocha; **Terminal** использует цвета вашего терминала.

Для PDF, DjVu и EPUB из изображений в списке также есть **Original** и **Inverted**. Темы чтения перекрашивают нейтральные бумагу и текст, сохраняя насыщенные цвета иллюстраций. Выбор записывается в `scan_theme`, доступный также через **F2 → Reader → PDF / image theme**. Для текстового чтения сохраняется отдельный `theme`.

**Картинки CBZ/CBR всегда сохраняют исходные цвета.** В комиксах **Space t** показывает предпросмотр только окружающего фона; **F2 → Reader → Comic background** меняет ту же настройку `comic_background`. Инверсия и перекрашивание PDF на комиксы не влияют.

**Space l f** переключает непрозрачный/прозрачный фон страницы; прозрачность всего окна задаётся в терминале.

### Текст, хайлайты и заметки

Выделяйте текст перетаскиванием, слово — двойным щелчком, абзац — тройным (в PDF — блок текста). Правый клик по слову или выделению открывает маленькое меню рядом с текстом: **Copy, Highlight, Search, Translate, Define, Add note**. Клик внутри выделения сохраняет весь отрывок. Escape или щелчок снаружи закрывает меню.

Нажмите на сохранённый хайлайт, чтобы открыть комментарий, либо выберите **Add note** в его контекстном меню. Выберите цвет мышью, введите текст и нажмите **Save** или **Ctrl+S**. Enter добавляет новую строку. Очистка комментария сохраняет хайлайт; удаление и текста, и цвета убирает пометку.

**Space a a** открывает список хайлайтов и заметок с поиском. Выбранная запись показывает содержимое и позицию в книге/разделе либо номер страницы; доступны **Go**, **Edit**, **Color**, **Delete** и **Export**. **Space a p** добавляет заметку к фиксированной странице. Заметки и хайлайты остаются доступны локально. Если открыть издание с подключённой полки BookOrbit, BOTUI синхронизирует комментарии к фрагментам, цвета и удаления в обе стороны: текст — по точным CFI-диапазонам, выделения PDF — по координатам страницы. Изменения без сети отправляются при следующем открытии этого издания через BookOrbit. **Space a s** показывает состояние обмена и предлагает **Sync now / retry unconfirmed uploads** — синхронизировать сейчас или повторить неподтверждённую отправку.

При одновременном редактировании сохраняются обе версии. Если ответ на создание пометки потерян, BOTUI сначала ищет уже созданную запись на сервере. Пометки без точного адреса остаются на исходной стороне; их количество видно в статусе. Заметки ко всей странице, выделения через несколько страниц PDF или документов внутри EPUB и форматы без подходящего CFI остаются локальными. При редактировании только комментария исходные браузерные стиль и цвет сохраняются на сервере; в BOTUI используется ближайший из пяти цветов выделения.

**Define** и **Translate** используют подключения из [настроек](#словари-и-перевод). Выбранный текст отправляется только по команде пользователя. Клавиатурные команды и движения Vim перечислены ниже.

## Управление

Нажмите **Space**, затем клавиши команды **по очереди**. Подсказка меню по умолчанию появляется через 150 мс; команды срабатывают сразу. В подменю **q / Esc / Backspace** возвращают назад; Escape закрывает корневое меню.

Книга открывается без курсора. **Space c** включает навигацию и выделение Vim; **v / V** выделяют только в этом режиме. Escape отменяет незавершённую команду или выделение, а из свободного Normal возвращает в обычное чтение. **Space c** также сразу возвращает в чтение. Пользовательские `[keys]` действуют в обычном чтении EPUB/FB2 и не заменяют команды Space и движения Vim.

<details>
<summary>Полный список команд чтения, Vim и окон</summary>

Набор команд зависит от документа: действия со словами требуют доступного текста; масштаб, поворот и заметки на страницу относятся к PDF, DjVu, комиксам и EPUB из изображений. Команды текстового ридера ниже относятся и к Markdown, если явно не указан формат. В обычном чтении q/Q возвращает к источнику; в Vim используйте Space q.

| Клавиши | Действие |
| --- | --- |
| `Space c` | Включить курсор Vim / вернуться в чтение |
| `Space m` | Меню чтения |
| `Space o` | Оглавление |
| `Space z` | Дзен-режим |
| `Space p` | Тусклая строка прогресса в дзене |
| `Space f` | Поиск в книге |
| `Space b a` | Поставить/снять закладку |
| `Space b l` | Список закладок |
| `Space a y` | Копировать выделение; в Normal — слово под курсором |
| `Space a h` | Цветное выделение |
| `Space a n` | Комментарий к фрагменту |
| `Space a a` | Выделения и заметки |
| `Space a e` | Экспорт пометок в Markdown |
| `Space a s` | Статус синхронизации пометок BookOrbit / повтор |
| `F9` / `Space r v` | Markdown: оформленный текст / исходник |
| `Space a v` | Показать/скрыть текст заметки (текстовый ридер) |
| `Space a p` | Заметка к странице (PDF/DjVu/комиксы) |
| `Space d d` | Значение слова |
| `Space d t` | Перевести выделенный текст |
| `Space l 1` | Одна страница |
| `Space l 2` | Разворот |
| `Space l 3` | Непрерывная прокрутка |
| `Space l w` | Ширина текста / зазор; масштаб и зазор фиксированных страниц |
| `Space l f` | Непрозрачный/прозрачный фон |
| `Space l u` | Стиль границ |
| `Space t` | Тема с предпросмотром; у фиксированных страниц также Original / Inverted |
| `Space s` | Настройки |
| `Space ?` | Справка |
| `q` / `Q` / `Space q` | Закрыть книгу и вернуться к её источнику |
| `Space r c` | Открыть обложку во внешнем просмотрщике |
| `Space r s` | Статистика чтения |
| `Space r [` | Вернуться к предыдущему месту |
| `Space r ]` | Перейти вперёд по истории |
| `Space r i` | Ссылки и иллюстрации (текстовый ридер) |
| `Space r f` | Перейти по видимым ссылкам (текстовый ридер) |
| `Space r p` | Номер страницы (PDF/DjVu/комиксы) |
| `Space r +` | Увеличить (PDF/DjVu/комиксы) |
| `Space r -` | Уменьшить (PDF/DjVu/комиксы) |
| `Space r 0` | Вписать страницу / по ширине в прокрутке (PDF/DjVu/комиксы) |
| `Space r r` | Повернуть по часовой стрелке (PDF/DjVu/комиксы) |
| `Space r R` | Повернуть против часовой стрелки (PDF/DjVu/комиксы) |

**Без префикса в обычном чтении**

| Клавиши | Действие |
| --- | --- |
| `← → / PgUp PgDn` | Листать страницы; стрелки также сдвигают увеличенные фиксированные страницы |
| `↑ ↓ / j k` | Перемещение по тексту; по строке в режиме прокрутки текста |
| `h l` | Предыдущая/следующая страница или горизонтальное перемещение |
| `1 / 2 / 3` | Одна страница / разворот / прокрутка; в Vim — счётчик |
| `+ / = / -` | PDF/DjVu/комиксы: увеличить / увеличить / уменьшить масштаб |
| `Home End` | Начало/конец (фиксированные страницы: документ; Vim: строка) |
| `Tab` | Фокус видимого оглавления; стрелки сворачивают/разворачивают, Enter открывает |
| `F1 / F2` | Справка / настройки |
| `F3 / F4 / F5` | Следующее / предыдущее совпадение / список результатов |
| `Esc` | Назад/закрыть окно; меню чтения |
| `Ctrl+C` | Копировать выделенный текст |

**Vim — после Space c**

| Клавиши | Действие |
| --- | --- |
| `Space c` | Войти в Normal; повторить для выхода в чтение из Normal или Visual |
| `v / V` | Visual / выделение логической строки |
| `Esc` | Отменить незавершённую команду или Visual; из свободного Normal → чтение |
| `h j k l` | Курсор влево/вниз/вверх/вправо; j/k идут по видимым строкам |
| `w b e` | Следующее слово / предыдущее / конец слова |
| `0 ^ $` | Начало логической строки / первый текст / конец |
| `{ }` | Предыдущий/следующий абзац (PDF: блок текста) |
| `gj gk / g0 g$` | Экранные строки / края экранной строки (текстовый ридер) |
| `gg / G` | Начало/конец текста (PDF: текущая страница) |
| `y` | Копировать выделение в Visual; начать оператор копирования в Normal |
| `yw ye yy y$` | Копировать до следующего слова / конца слова / строку / до конца строки |
| `yiw yaw yip yap` | Копировать слово / слово с пробелом / абзац |
| `3j / 3yw / y3w` | Повторить движение или копирование; число можно поставить перед оператором или движением |
| `viw / vaw / vip / vap / gv` | Выделить слово / слово с пробелом / абзац / восстановить предыдущее выделение |
| `/ ? / n N` | Поиск вперёд/назад / следующее/предыдущее совпадение |
| ``ma / 'a / `a`` | Поставить именованную метку a / вернуться к ней |
| `Ctrl+O / Ctrl+I` | Назад / вперёд по истории |
| `K` | Значение слова/выделения |

**Открытые окна и редакторы**

| Клавиши | Действие |
| --- | --- |
| `↑ ↓ / Enter / click` | Выбрать и открыть пункт |
| `Esc` | Закрыть окно; редактор настроек предложит сохранить/отбросить правки |
| `Ctrl+S / Enter` | Редактор заметки: сохранить / новая строка; очистка текста сохраняет хайлайт; отсутствие текста и цвета удаляет пометку |
| `Tab` или клик по цвету | Редактор заметки: цвет выделения; ✓ отмечает выбранный цвет |
| `[ ] / { }` | Настройка текста: ширина страницы / расстояние между колонками |
| `/ / N / H / Y / Del / E` | Список пометок: поиск / изменить / цвет / копировать / удалить / экспорт |
| `P / L / S / Y / N` | Ответ словаря: подключение / язык ответа / исходный язык / копировать / сохранить заметкой |

</details>

<details>
<summary>Библиотеки, карточки и запуск из командной строки</summary>

| Экран | Клавиши |
| --- | --- |
| Начальный экран | Enter/L — локальная библиотека; B — BookOrbit; P — OPDS; D — офлайн BookOrbit; O — загрузки OPDS; Q — выход |
| Полки | / — поиск; V — сетка/список; U — границы; F — фон; Enter/щелчок — открыть; Q/Esc — назад; F6 — левая панель |
| BookOrbit | 0 — дашборд; 1 — книги; 2 — читаемые; 3 — недавние; 4 — библиотеки; 5 — коллекции; 6 — авторы; 7 — серии |
| Локальная библиотека | 1 — папки; 2 — читаемые; 3 — недавние; 4 — коллекции; P — выбрать папку; R — сделать её корнем; A — состав коллекций; I — сведения о файле |
| Коллекции | BookOrbit: N создаёт коллекцию; локальные: New collection в разделе 4; Del удаляет выбранную коллекцию с подтверждением |
| Карточка BookOrbit | R — читать; D — сохранить; C/обложка — просмотр обложки; I — аннотация; H — отложить/возобновить; X — сбросить чтение; A — коллекции; L — журнал; B — похожие книги |
| Офлайн-загрузки | Del — удалить скачанную копию с подтверждением |

```sh
botui [FILE | DIRECTORY]
botui --local [DIRECTORY]
botui --library DIRECTORY
botui --resume
botui --server URL
botui --opds URL
botui --config-path
botui --check-bookmarks TITLE
botui --help
botui --license
```

</details>

## Настройки

**F2** изменяет настройки в том же TOML-файле, который можно редактировать вручную. Команда `botui --config-path` показывает его расположение. Чтение, оформление, библиотеки, BookOrbit и словари настраиваются вместе; сохранение не стирает посторонние значения и комментарии.

### Словари и перевод

В **F2 → Dictionaries** есть отдельные списки **Word definitions** и **Translation**. Добавьте, измените, проверьте, удалите или выберите подключение по умолчанию, затем нажмите **Save changes & back**. Free Dictionary и Google Translate включены в исходном конфиге обычными редактируемыми HTTP-профилями. Они не зашиты в код как отдельные провайдеры; их можно заменить или убрать. Явно пустой список остаётся пустым.

Выберите подходящий тип подключения:

| Тип | Настройка |
| --- | --- |
| HTTP API | `provider = "http"`, URL, параметры запроса и путь к тексту в ответе |
| OpenAI-совместимый API | `provider = "openai"`, адрес chat completions и модель; при необходимости — ключ и язык объяснения |
| Локальная команда | `provider = "command"` и массив программы с аргументами; программу и её словари нужно установить отдельно |

`[lookup]` хранит имена выбранных `dictionary` и `translation` и общие языковые настройки. Каждый `[[lookup.profiles]]` описывает одно подключение; его уникальное `name` используется для выбора по умолчанию. Пустое имя выбирает первый профиль соответствующего назначения. Перед добавлением блоков профилей удалите `profiles = []`.

Короткий и длинный профили используют **одну схему**: длинный явно задаёт необязательные значения или параметры конкретного сервиса. Для HTTP:

| Поле | Значение |
| --- | --- |
| `name`, `provider`, `endpoint` | Имя подключения, `"http"`, полный URL сервиса |
| `purpose` | По умолчанию `"dictionary"`, для перевода — `"translation"` |
| `method`, `response_format` | По умолчанию `"GET"` и `"json"`; можно опустить, если они подходят |
| `response_path` | Путь к строкам JSON, например `/translatedText` или `/*/shortdef/*`; `*` перебирает массивы. Пустой путь выбирает корень. |
| `response_separator` | По умолчанию без разделителя; `"\n"` выводит определения отдельными строками |
| `query`, `headers`, `body` | Только те поля запроса, которые нужны сервису |
| `api_key` / `key_command` | Значение ключа или команда, которая его возвращает; команда имеет приоритет. HTTP передаёт ключ только в местах с `{key}`. |
| `description` | Необязательная подпись для себя |

Подстановки: `{text}`, `{source}`, `{target}`, `{key}`. URL и query кодируются автоматически; в JSON-теле подстановки помещаются внутрь строк. Выбор языка влияет только на шаблоны, которые его используют: английский словарь не становится переводчиком от выбора русского языка. Для обычного текстового ответа задайте `response_format = "text"`.

Полный пример ниже совпадает с [config.example.toml](config.example.toml). Free Dictionary и Google Translate активны; Merriam-Webster Collegiate, произвольный переводчик, OpenAI-совместимый сервер и `sdcv` даны закомментированными шаблонами. Для Merriam-Webster нужен ключ продукта **Collegiate**; шаблон извлекает краткие определения (`shortdef`). Раскомментируйте блок, укажите ключ и выберите имя профиля в `lookup.dictionary` либо заполните те же поля через F2. **Test connection with ‘hello’** отправляет настоящий запрос.

Старые сервисные профили однократно преобразуются в явные HTTP-поля. Недостающие настройки добавляются в старый конфиг без перезаписи существующего выбора. Дополнительные поля подключений, подстановки языков и примеры получения пароля описаны в самом конфиге.

<details>
<summary>Полный пример config.toml</summary>

```toml
# Local library. Uncomment to use a specific folder.
# library_root = "/path/to/books"
local_grid = false
# Width of the navigation sidebar on the bookshelf, in cells. Omit for Auto.
# sidebar_width = 28
# Width of the contents sidebar in an open book, in cells. Omit for Auto.
# contents_width = 24
# Per downloaded edition, in MiB (BookOrbit and OPDS). 0 removes the limit.
max_download_mib = 4096

# Reader defaults. Use "terminal" to follow your terminal's colors.
theme = "terminal"
# PDF/DjVu and image-only EPUB: original, inverted, terminal, or a reading
# palette ID such as sepia-light or ember-dark. F2 and Space t save the same choice.
scan_theme = "original"
# CBZ/CBR background only; artwork always retains its original colors.
# Use terminal or a reading palette ID. F2 and Space t save the same choice.
comic_background = "terminal"
interface_theme = "catppuccin-mocha"
# Optional override. By default, solid Terminal paper follows the active terminal background.
# opaque_color = "#1f2125"
zen = false
# Show a dim page count and percentage in zen mode. Toggle with Space p.
zen_progress = false
# PDF/DjVu zoom in percent. In scroll mode 100 fits the reading pane.
scan_zoom = 100
# Space between PDF/DjVu/comic pages in a spread (0–32 terminal cells).
scan_gap = 2
# Preferred width of one text page and the space between facing pages, in terminal cells.
# page_width = 78
column_gap = 4
# First line of a prose paragraph in EPUB/FB2, in cells (0–8; 0 disables).
paragraph_indent = 0
# Blank lines between EPUB/FB2 paragraphs (0, 1 or 2). Omit to follow the book.
# paragraph_spacing = 1
# Default layout for books without a saved mode: auto, single, spread, scroll.
reading_layout = "auto"
opaque_background = false
# Window frames, cover cards and jackets: "rounded", "square", or "none".
borders = "rounded"
preserve_printed_lines = false
hide_inline_images = false
# Lines per mouse-wheel tick in continuous scroll (EPUB/FB2/PDF/DjVu, 1–12).
scroll_lines = 1
# Delay before the Space command menu appears. Fast sequences execute immediately.
leader_delay_ms = 150
# Legacy lookup_command is still read; use lookup.profiles for new configurations.

[bookorbit]
# Remember which navigation groups are folded on a wide screen.
# libraries_collapsed = false
# collections_collapsed = false
server = ""
username = ""
list_view = false
# Uncomment one way to sign in with this account.
# password_command = ["pass", "show", "bookorbit"]
# password = "your password"

# Reader commands use Space; Space c toggles the optional Vim cursor.
# Direct reader navigation and Vim mappings are documented in README and F1.
# Optional explicit single-letter overrides for EPUB/FB2 reading only.
# They never override Space commands or Vim motions. No overrides by default.
[keys]
# x = "next"
# Supported action names:
# next, previous, menu, zen, zen-progress, background, borders, cover, resources,
# statistics, note, notes, export-notes, jump-back, jump-forward, theme, contents,
# search, search-results, next-match, previous-match, bookmark, bookmarks,
# single, spread, scroll, page-layout, wider-gap, narrower-gap,
# wider-margins (alias narrower-page), narrower-margins (alias wider-page), close

# Multiple OPDS catalogs. Credentials are optional for public catalogs.
# [[opds]]
# name = "My catalog"
# url = "https://books.example.org/opds"
# username = "reader"
# password_command = ["pass", "show", "books/opds"]
# password = "plain-text alternative"

# F2 → Dictionaries has two independent lists: Word definitions and Translation.
# Add/edit/delete a profile in its list, then Save changes & back.
# These names select the default connection in each list; names must be unique.
[lookup]
dictionary = "English dictionary"
translation = "Google Translate"
source_language = "auto"
target_language = "ru"

# Active defaults are ordinary editable HTTP profiles, not built-in providers.
# Remove a block to remove a connection. Set profiles = [] under [lookup]
# only if there are no [[lookup.profiles]] blocks and you want an empty list.
[[lookup.profiles]]
name = "English dictionary"
purpose = "dictionary"
provider = "http"
endpoint = "https://api.dictionaryapi.dev/api/v2/entries/en/{text}"
response_path = "/*/meanings/*/definitions/*/definition"
response_separator = "\n"

[[lookup.profiles]]
name = "Google Translate"
purpose = "translation"
provider = "http"
endpoint = "https://translate.googleapis.com/translate_a/single"
query = { client = "gtx", sl = "{source}", tl = "{target}", dt = "t", q = "{text}" }
response_path = "/0/*/0"
# Translation segments must join without extra separators.
response_separator = ""

# Additional recipes below are inactive. Add your own in F2 > Dictionaries or below.
# The active Google endpoint is unofficial; availability is not guaranteed.
# Minimal HTTP profile: name, provider, endpoint, response_path for nested JSON.
# purpose defaults to "dictionary"; method defaults to GET; response_format to json.
# description, query, headers, body, api_key and key_command are optional.
# response_separator defaults to "" (concatenation); use "\n" for definitions.
# command profiles need name, provider = "command" and command instead of HTTP fields.
# OpenAI profiles need name, provider = "openai" and model; endpoint has a local default.
# name is unique; description is optional and editable. purpose selects the group.
# HTTP templates: {text}, {source}, {target}, {key}. URL/query values are encoded;
# JSON string values are escaped safely. Headers are passed as configured.
# response_path uses /object/key/0 and * for array items. Empty = JSON root.
# response_format = "text" reads a plain UTF-8 response instead of JSON.
# Supported methods: GET (default), POST, PUT, PATCH, DELETE.
# Optional JSON source: "{source?}" omits its object field for auto/empty source.
# {source:upper}/{target:upper} uppercase language codes; {source:upper?} combines both.

# Example only: an English definitions API, not an installed connection.
# [[lookup.profiles]]
# name = "My English dictionary"
# description = "English definitions"
# purpose = "dictionary"
# provider = "http"
# endpoint = "https://api.dictionaryapi.dev/api/v2/entries/en/{text}"
# method = "GET"
# response_format = "json"
# response_path = "/*/meanings/*/definitions/*/definition"
# response_separator = "\n"

# Merriam-Webster Collegiate: short definitions, not full entries or audio.
# Get a key for the Collegiate product: https://dictionaryapi.com/register/index
# Official schema: https://dictionaryapi.com/products/json#sec-2.shortdef
# [[lookup.profiles]]
# name = "Merriam-Webster Collegiate"
# purpose = "dictionary"
# provider = "http"
# endpoint = "https://www.dictionaryapi.com/api/v3/references/collegiate/json/{text}"
# query = { key = "{key}" }
# api_key = "YOUR-COLLEGIATE-API-KEY"
# response_path = "/*/shortdef/*"
# response_separator = "\n"
# Alternative to api_key: key_command = ["pass", "show", "dictionaries/collegiate"]
# If the API returns spelling suggestions instead of entries, this path has no text.
# It reports "No text at the configured response path"; try another spelling.

# Example only: a custom JSON translation API. Match your service's schema.
# [[lookup.profiles]]
# name = "My translator"
# purpose = "translation"
# provider = "http"
# endpoint = "http://localhost:5000/translate"
# method = "POST"
# query = { version = "1" }
# headers = { Authorization = "Bearer {key}" }
# body = '{"q":"{text}","source":"{source}","target":"{target}"}'
# response_format = "json"
# response_path = "/translatedText"
# response_separator = ""
# key_command = ["pass", "show", "translation/api"]
# api_key = "plain-text alternative"
# key_command takes precedence over api_key. Keep only the method you use.

# OpenAI-compatible chat completions, including a locally hosted server.
# [[lookup.profiles]]
# name = "My explanations"
# description = "Explain the selected word"
# purpose = "dictionary" # or "translation"
# provider = "openai"
# endpoint = "http://localhost:11434/v1/chat/completions"
# model = "your-installed-model"
# target_language = "ru" # Explanation language; empty uses lookup.target_language.
# api_key = "optional key"

# Offline dictionary program (install it and its dictionaries separately).
# [[lookup.profiles]]
# name = "My local dictionary"
# purpose = "dictionary"
# provider = "command"
# command = ["sdcv", "-n", "{text}"]
# Arguments are passed without a shell. Also available: {source}, {target}.
```

</details>

## Разработка

| Команда | Что проверяет |
| --- | --- |
| `mise run check` | Форматирование, Clippy и тесты Rust |
| `mise run smoke` | Работу библиотеки и читалки в изолированных терминальных сеансах |

Для smoke-тестов нужны tmux, Python 3.11+, указанные выше средства чтения PDF и DjVu, а также `c44` и `djvm` для создания тестовых файлов.

[Архитектура](docs/architecture.ru.md) · [Проверяемые сценарии](docs/checklist.ru.md) · [Границы проверок](docs/checklist.ru.md#границы-проверок)

[История изменений](CHANGELOG.md)

## Лицензия

BOTUI и собственные обложки-заглушки распространяются под [GPL-3.0-or-later](LICENSE). Права на обложки из книг и каталогов сохраняются за их правообладателями.

Декодер RAR [rars](https://github.com/bitplane/rars) распространяется под [Apache-2.0](docs/licenses/rars-Apache-2.0.txt). Уведомления о компонентах доступны в **F2 → Legal**, через `botui --license` и в [docs/licenses](docs/licenses/).

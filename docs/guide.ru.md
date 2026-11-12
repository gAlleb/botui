# Руководство BOTUI

[English](guide.md) · **Русский**

[README](../README.ru.md)

## Первый запуск

```sh
botui                                       # выбрать источник
botui --library "$HOME/Books"               # запомнить папку библиотеки
botui --local                               # вернуться в неё
botui "./examples/first-light.fb2"          # открыть файл напрямую
botui --resume                             # продолжить последнюю незавершённую локальную книгу
botui --server https://books.example.org    # корень установки BookOrbit
botui --opds https://books.example.org/opds
botui --config-path                         # создать конфиг и вывести его путь
```

При запуске выделена локальная библиотека. **Enter** открывает её. На стартовом экране перемещайте выделение, чтобы увидеть описание выбранного источника и подсказки. **B** открывает BookOrbit, **L** — локальную папку, **P** — OPDS, **D** — офлайн-книги BookOrbit, **O** — загрузки OPDS.

Выбранный пункт также открывается клавишей Enter или щелчком.

Щёлкните по обложке **в карточке книги** или нажмите **C**, чтобы открыть её в системном просмотрщике. На полке щелчок по-прежнему открывает карточку. В локальном EPUB или FB2 клавиша **Space r c** открывает встроенную обложку.

Если у книги или каталога нет картинки, BOTUI рисует [переплёт](images/generated-jacket.png) с названием и именем автора. В PDF и DjVu клавиша **Space r c** показывает первую страницу. BOTUI использует системную ассоциацию изображений через `xdg-open`.

**F2** открывает настройки: Appearance, Reader, Library, BookOrbit, Dictionaries, Keys, About и Legal. **F1** (или **?** на полке) открывает справку для текущего экрана. Внизу есть кликабельные подсказки.

**Q** на полках и в обычном режиме чтения возвращают на один шаг назад. В BookOrbit: чтение → карточка книги → полка, дашборд или поиск, откуда вы открыли книгу. В локальной библиотеке: чтение → папка → родительские папки → корень библиотеки → выбор источника. При возврате сохраняются выделение и прокрутка. На стартовом экране **Q** завершает BOTUI. При запуске с путём к отдельному файлу закрытие книги завершает команду.

**Esc** также возвращает назад на экранах библиотеки, а в читалке открывает меню чтения. В строке поиска **Q** вводится в запрос, **Esc** закрывает поиск. Кнопка **Home** на полке BookOrbit сразу возвращает к выбору источника.

В справке **?** клавиши **[ / ]** переключают разделы, а **/** ищет по клавише или описанию.

### Локальная библиотека

**Local library** открывает сохранённую папку, затем пробует `~/Books` и текущий каталог. Через **P Folder…** можно вставить путь. Кнопка **R Set library here** запоминает текущую папку как корень.

**/** открывает окно поиска по центру полки, ищет по всей библиотеке и показывает результаты уже при вводе первой буквы. Можно пропускать буквы и диакритику: `желз амб` находит Желязны и Амбер, `kukai` — Kūkai.

Сначала доступны имена файлов и подпапок; пока вы печатаете, добавляются названия, авторы и серии из EPUB/FB2. **↑/↓** выбирают книгу, **Enter** открывает её, **Esc** возвращает на полку.

**2** показывает историю чтения с процентами, **3** — книги по дате создания файла, а при её отсутствии по дате изменения.

Слева расположены дерево папок, «Сейчас читаю» и «Недавно добавленные». Щелчок по имени папки показывает её книги справа, по стрелке — раскрывает или сворачивает ветку. **F6** переключает фокус, **← / →** сворачивают и раскрывают, **Enter** открывает папку. Границу можно потянуть мышью.

В небольшом окне остаются вкладки с номерами. [Локальная библиотека в Kitty](images/local-library.png).

## Управление коллекциями

| Действие | Локальная библиотека | BookOrbit |
| --- | --- | --- |
| Создать коллекцию | Нажмите **4**, выберите **New collection…**, введите имя и нажмите **Enter**. | Нажмите **5**, затем **N**, введите имя и нажмите **Enter**. |
| Добавить книгу | Выделите файл, нажмите **A** и выберите **Add to** рядом с названием коллекции. | Откройте карточку книги, нажмите **A Collections** и выберите **Add to**. |
| Убрать книгу | Выделите файл, нажмите **A** и выберите **Remove from**. | В карточке книги нажмите **A Collections** и выберите **Remove from**. |
| Удалить коллекцию целиком | Нажмите **4**, откройте коллекцию и нажмите **Del**. | Нажмите **5**, выделите свою коллекцию и нажмите **Del**. Это работает и внутри открытой коллекции. |

**Del Delete collection** доступна и мышью. При подтверждении проверьте название и выберите **Delete collection**. Изначально выделена **Cancel**.

Одна книга может входить в несколько коллекций. После удаления коллекции книги остаются в библиотеке и в остальных коллекциях. Локальные коллекции хранятся на устройстве. Изменения коллекций BookOrbit сохраняются в аккаунте и видны в веб-интерфейсе после обновления страницы.

## Полка BookOrbit

Выберите **Connect to BookOrbit** и войдите. На дашборде доступны продолжение чтения, последние поступления и рекомендации. Когда сервер возвращает соответствующие виджеты, появятся также серия дней чтения, годовая цель, задание месяца и Reading DNA.

Щелчок по обложке открывает выбор файла издания. Нажмите **0**, чтобы вернуться на дашборд и обновить данные.

| Клавиша | Раздел | Клавиша | Раздел |
| --- | --- | --- | --- |
| 0 | Dashboard — главная | 4 | Libraries — библиотеки |
| 1 | All books — все книги | 5 | Collections — коллекции |
| 2 | Currently reading — читаю сейчас | 6 | Authors — авторы |
| 3 | Recently added — последние поступления | 7 | Series — серии |

Рядом с **Authors** и **Series** указано полное количество авторов и серий, доступных аккаунту. Внутри каждого раздела видно число книг у автора или в серии. Список томов учитывает порядок серии на сервере. **/** открывает по центру полки окно с результатами поиска.

Список обновляется при вводе; **Esc** или щелчок за пределами окна закрывает поиск. В разделах **Authors** и **Series** поиск идёт по открытому справочнику. На дашборде и книжных полках он ищет по названию, автору и серии во всём доступном каталоге BookOrbit.

Например, `желз` находит Желязны, а `znak edin` — «Знак Единорога». Список пополняется, пока BOTUI получает метаданные с сервера. Выберите строку стрелками **↑/↓** и нажмите **Enter** или щёлкните по результату. **Esc** из карточки возвращает к тому же запросу и выбранной строке.

**V** переключает обложки и список на дашборде и на полках. Выбранный вид сохраняется и действует в обоих местах. **Tab / Shift+Tab** проходят по разделам в том же порядке, что и цифры. Книги листаются стрелками, колесом, Page Up / Page Down. Следующие записи загружаются автоматически. Enter или щелчок открывают выбранное, **Esc** возвращает назад.

### Офлайн-чтение и синхронизация

При первом нажатии **Read** BOTUI скачивает издание BookOrbit и сохраняет его на устройстве. При повторном открытии с карточки читалка берёт файл с диска, а позицию и текстовые закладки получает из аккаунта. **D Save offline** позволяет скачать книгу заранее или проверить обновление.

Если BookOrbit сообщил об изменении издания, читалка проверяет версию файла на сервере.

Когда книга открыта через BookOrbit, BOTUI отправляет позицию через две секунды после остановки на странице. При возвращении в окно терминала он проверяет, где вы остановились на другом устройстве. **Go to BookOrbit position** переносит туда, **Stay here** оставляет на текущем месте. Пока книга открыта и вы не листаете, проверка также проходит каждые 15 секунд. Открытое предложение продолжает обновляться. При выборе **Go to BookOrbit position** читалка ещё раз запрашивает последнюю сохранённую позицию.

Без сети откройте **BookOrbit offline downloads** на стартовом экране. Если вы закрыли EPUB онлайн на пятой главе и дошли офлайн до седьмой, при закрытии читалка запомнит седьмую главу.

При действующем подключении к аккаунту или настроенном пароле (включая команду для его получения) BOTUI сам повторяет синхронизацию, пока программа запущена. Место сохраняется и после перезапуска. Если вы вводите пароль вручную, достаточно снова войти в аккаунт.

Если позиции разошлись, открой скачанное издание через библиотеку BookOrbit. Выбери **Use offline position**, чтобы отправить своё место на сервер, или **Use BookOrbit position**, чтобы продолжить с серверной позиции. **Esc** позволяет отложить выбор. На полке загрузок BookOrbit или OPDS выделите файл и нажмите **Del**, чтобы удалить его копию с устройства. Книга останется на сервере. BOTUI сохранит ещё не отправленную позицию BookOrbit для следующей синхронизации.

Офлайн-закладки хранятся локально. Для книг из каталогов OPDS есть отдельная полка **OPDS downloads** с локальным прогрессом.

На широком экране дерево навигации имеет собственную рамку. Щёлкните **Libraries** или **Collections**, чтобы свернуть группу, либо нажмите **F6** и используйте **↑/↓** и **←/→**. Клавиши **4 / 5** открывают весь соответствующий раздел; свёрнутые группы запоминаются.

Карточка до открытия файла показывает автора, серию, аннотацию, издателя, год, язык, ISBN и ваш прогресс, если сервер их вернул. **R** читает выбранное издание, **D** сохраняет его офлайн, **I** раскрывает аннотацию.

**H Put on hold** убирает книгу из активного чтения и сохраняет позицию; **H Resume reading** возвращает её. **X Reset reading state** с отдельным подтверждением обнуляет все издания этой книги в вашем BookOrbit и переводит её в Unread. Закладки, заметки и файлы сохраняются.

Читалка отправляет 100% при достижении конца; статус назначается правилами BookOrbit. Проверить доступность адресов закладок по изданиям без изменения позиции можно командой `botui --check-bookmarks "Название"`.

Так выглядят [карточка книги до чтения](images/book-details.png) и [навигация со свёрнутыми библиотеками](images/navigation-folded.png).

**L Reading log** показывает даты, устройства, длительность и прогресс сеансов, которые зарегистрировал BookOrbit. **B Related books** открывает книги автора или серии. Таймер **S** отдельно считает время в BOTUI на этом устройстве.

### Каталоги OPDS

**OPDS** принимает OPDS 1 (Atom) и OPDS 2 (JSON). Поддерживаются навигация, группы, фасеты, обложки, выбор издания, прямые загрузки, поиск по объявленному шаблону и OpenSearch. Каталоги с HTTP Basic используйте через HTTPS.

Книги, для которых доступны только выдача или DRM, отображаются без кнопки прямой загрузки. Позиции и заметки хранятся на устройстве.

## Чтение

Книга открывается без курсора. **Space** открывает команды; подсказка появляется через **150 мс**, но следующую клавишу можно нажать сразу. Это последовательность, не одновременное нажатие. `Space c` включает Vim. Только тогда `v`/`V` выделяют текст. В обычном чтении старые буквы `t`, `b`, `v` и т. п. не выполняют команды. Явные пользовательские `[keys]` — исключение для EPUB/FB2; они не меняют Vim и Space.

В подменю Space `q`, `Esc` и `Backspace` возвращают на первый уровень; `Esc` закрывает корневое меню. Библиотеки и редакторы имеют собственные клавиши, перечисленные отдельно.

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
| `Space a v` | Показать/скрыть текст заметки (EPUB/FB2) |
| `Space a p` | Заметка к странице (PDF/DjVu) |
| `Space d d` | Значение слова |
| `Space d t` | Перевести выделенный текст |
| `Space l 1` | Одна страница |
| `Space l 2` | Разворот |
| `Space l 3` | Непрерывная прокрутка |
| `Space l w` | Ширина страниц / зазор; масштаб PDF |
| `Space l f` | Непрозрачный/прозрачный фон |
| `Space l u` | Стиль границ |
| `Space t` | Выбор темы с предпросмотром; в PDF также Original / Inverted |
| `Space s` | Настройки |
| `Space ?` | Справка |
| `q` / `Q` / `Space q` | Закрыть книгу и вернуться к её источнику |
| `Space r c` | Открыть обложку во внешнем просмотрщике |
| `Space r s` | Статистика чтения |
| `Space r [` | Вернуться к предыдущему месту |
| `Space r ]` | Перейти вперёд по истории |
| `Space r i` | Ссылки и иллюстрации (EPUB/FB2) |
| `Space r f` | Перейти по видимым ссылкам (EPUB/FB2) |
| `Space r p` | Номер страницы (PDF/DjVu) |
| `Space r +` | Увеличить (PDF/DjVu) |
| `Space r -` | Уменьшить (PDF/DjVu) |
| `Space r 0` | Вписать страницу / по ширине в прокрутке (PDF/DjVu) |
| `Space r r` | Повернуть по часовой стрелке (PDF/DjVu) |
| `Space r R` | Повернуть против часовой стрелки (PDF/DjVu) |

**Без префикса в обычном чтении**

| Клавиши | Действие |
| --- | --- |
| `← → / PgUp PgDn` | Листать страницы; в PDF стрелки также сдвигают увеличенный лист |
| `↑ ↓ / j k` | Перемещение по тексту; по строке в режиме прокрутки текста |
| `h l` | Предыдущая/следующая страница или горизонтальное перемещение |
| `1 / 2 / 3` | Одна страница / разворот / прокрутка; в Vim — счётчик |
| `+ / = / -` | PDF/DjVu: увеличить / увеличить / уменьшить масштаб |
| `Home End` | Начало/конец (PDF: документ; Vim: строка) |
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
| `gj gk / g0 g$` | Экранные строки / края экранной строки (EPUB/FB2) |
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
| `[ ] / { }` | Настройка страницы: ширина страницы / расстояние между колонками |
| `/ / N / H / Y / Del / E` | Список пометок: поиск / изменить / цвет / копировать / удалить / экспорт |
| `P / L / S / Y / N` | Ответ словаря: подключение / язык ответа / исходный язык / копировать / сохранить заметкой |

Клик по сохранённому хайлайту или заметке в книге открывает её комментарий. Правый клик внутри существующего хайлайта → **Add note** добавляет текст к той же пометке, сохраняя весь исходный отрывок. Перетаскивание мышью по-прежнему выделяет текст. В редакторе нажмите на кнопку цвета (выбранный отмечен ✓), затем **Save**; **Cancel** отменяет изменения. Очистка одного комментария оставляет цветное выделение. Для удаления всей пометки используйте **Delete** в списке.

В списке пометок показана позиция каждого отрывка: процент книги для EPUB/FB2, процент и страница/всего для PDF. Для выбранной записи EPUB/FB2 дополнительно видны название раздела и процент внутри него, если в книге есть оглавление. Это позиция начала пометки, а не текущий прогресс чтения; размер окна на неё не влияет. Клик по строке показывает содержимое, **Edit** открывает редактирование, **Color** — выбор цвета, **Go** — переход к отрывку.

### Таблицы, формулы и иллюстрации

Откройте учебник в EPUB так же, как роман. Читалка применяет стили из книги и показывает объединённые ячейки, соседние колонки, списки и строки кода. Формулы MathML и размеченный в книге TeX видны по ходу чтения. Например, у дроби числитель стоит над знаменателем. Чтобы дать широкой таблице больше места, нажмите **Space l w** и увеличьте ширину страницы.

Шрифт и размер ячейки задаёт терминал, цвет текста и бумаги — тема читалки. В абзацах с выравниванием по ширине между словами остаются обычные пробелы. Заголовки по центру и строки у правого края сохраняют своё выравнивание.

Поиск, сноски, закладки и возврат к позиции BookOrbit работают по исходному тексту, в том числе после переноса таблицы при смене ширины.

Встроенные растровые иллюстрации подстраиваются под высоту страницы. **Space r i** открывает иллюстрацию отдельно. В **F2 → Reader → Illustrations** можно оставить только отдельный просмотр.

В обычных абзацах FB2 сохраняются курсив, жирное и зачёркнутое выделение. Они не меняют текст поиска и адрес сохранённой позиции.

### Работа с текстом

Выделяйте мышью: перетаскивание — фрагмент, двойной щелчок — слово, тройной — абзац. Для клавиатуры включите `Space c`, затем `v`. `Space a y` копирует, `Space a h` ставит выделение, `Space a n` открывает комментарий. В Normal доступны операторы `y`, например `yiw` и `yip`.

`Space a a` открывает список пометок. В нём Del удаляет, N редактирует, H меняет цвет, Y копирует цитату, / ищет, E экспортирует. В редакторе Enter добавляет строку, Ctrl+S сохраняет; очистка текста сохраняет хайлайт; отсутствие текста и цвета удаляет пометку. Esc отменяет редактирование.

Пометки сохраняются на этом устройстве. Синхронизация диапазонных выделений с BookOrbit пока не реализована. Для PDF нужен текстовый слой; OCR не выполняется.

### Словари, перевод и подключения OPDS

**F2 → Dictionaries** разделяет **Word definitions** (значения слов) и **Translation** (перевод). Free Dictionary и Google Translate включены обычными HTTP-профилями в исходном TOML; их можно изменить или удалить. При добавлении выбирается способ подключения: **HTTP API**, **OpenAI-compatible API** или **Local command**. Название и необязательное описание задаёшь сам. Подключение можно изменить, проверить, выбрать основным или удалить. После удаления последнего подключения список остаётся пустым и после перезапуска.

У HTTP API настраиваются адрес, метод, параметры запроса, заголовки, JSON-тело и путь к тексту ответа. Шаблоны используют `{text}`, `{source}`, `{target}` и `{key}`. Путь `/items/*/text` собирает строки из элементов массива; разделитель объединяет результаты. Поддерживается и обычный текстовый ответ. Для других протоколов и способов авторизации можно подключить локальную команду. Примеры сервисов находятся в config.example.toml, а не в меню добавления. Старые конфигурации с названиями провайдеров продолжают читаться.

**Save changes & back** сохраняет изменения. При выходе из изменённой формы можно сохранить, отменить изменения или продолжить редактирование. Пустое описание не занимает место над формой. Пароли скрыты. Языки перевода находятся под списком переводчиков; **Explanation language** — в настройках конкретного OpenAI-совместимого словаря. В окне результата: **P** — подключение, **L/S** — язык результата/исходный язык, **Y** — копировать, **N** — сохранить заметку, **Esc** — закрыть. Задержка подсказки Space находится в **F2 → Reader**, по умолчанию 150 мс; команды выполняются сразу.

Запрос выполняется только по вашей команде, с лимитом 20 секунд и ограниченным кэшем ответов в памяти. Во внешний сервис уходит выбранное слово или фрагмент. Автоматического перевода в фоне и скрытой замены провайдера нет. Esc закрывает окно; уже отправленный HTTP-запрос может завершиться в фоне. Ключ можно задать текстом или командой получения ключа; команда имеет приоритет. В настройках ключ скрыт.

Все настройки находятся в одном `config.toml`: `[lookup]`, `[[lookup.profiles]]`, `[[opds]]`, `[bookorbit]`, параметры чтения и оформления. [Пример конфига](../config.example.toml) содержит подключения и способы авторизации. Меню сохраняет посторонние поля и комментарии. Старый `lookup_command` совместим.

**F2 → Library → OPDS catalogs** — именованные каталоги с адресами и необязательными паролями. Они появляются при выборе OPDS на стартовом экране. `botui --opds URL` использует сохранённое подключение с тем же URL. Публичному каталогу пароль не нужен.

### PDF и DjVu

`Space l 3` включает непрерывную прокрутку. `Space l w` открывает ширину/масштаб с кнопками; `Space r +` и `Space r -` изменяют масштаб, включая уменьшение ниже 100%. При 100% в прокрутке лист занимает ширину области чтения. `Space r 0` возвращает вписывание, `Space r p` открывает небольшое окно номера страницы.

`Space t` открывает компактный выбор темы поверх страницы, включая Original и Inverted. `Space l f` меняет прозрачность. `Space o` открывает дерево оглавления; стрелки сворачивают и разворачивают разделы. Между листами в прокрутке — тусклый пунктир во всю ширину.

Для PDF с текстовым слоем доступны выделение, копирование и пометки. Выделение привязано к словам и сохраняет координаты при масштабе и повороте. Порядок извлечения текста может отличаться от визуального в сложных многоколоночных PDF. `Space a p` создаёт заметку к странице PDF/DjVu. `Space f` ищет в текстовом слое.

BOTUI сохраняет положение внутри страницы локально; BookOrbit получает номер страницы. EPUB из сканов открывается в этом же просмотрщике.

## Настройки

Команда `botui --config-path` выводит точный путь. По умолчанию это `~/.config/botui/config.toml`. В одном файле находятся локальная папка, оформление, привязки клавиш и подключение к BookOrbit. Интерфейс по умолчанию английский. См. [пример со всеми настройками](../config.example.toml).

При запуске в старый конфиг добавляются отсутствующие настройки словарей и перевода, задержки Space, шага колеса и пустой список OPDS. Существующие значения, подключения и комментарии сохраняются.

При первом запуске интерфейс использует **Catppuccin Mocha**.

В **F2 → Reader** настройка **First-line indent** сдвигает первую строку абзаца вправо на 0–8 ячеек. **Paragraph spacing** задаёт вертикальный промежуток между абзацами в EPUB и FB2. По умолчанию **From book** берёт интервал из разметки EPUB, а в обычной прозе FB2 оставляет одну пустую строку. Можно выбрать свой интервал в 0, 1 или 2 пустые строки. Выбор сразу применяется к открытой книге.

Чтобы изменить ширину оглавления в открытой книге, потяните мышью вертикальную границу между списком глав слева и текстом. Ширину также можно ввести в **F2 → Reader → Contents width**. На книжной полке тяните границу между навигацией и списком книг или откройте **F2 → Library → Sidebar width**.

**F2 → Library → Download limit** задаёт предел на одно издание BookOrbit или OPDS: по умолчанию **4096 МиБ**, **0** снимает ограничение. Книга скачивается частями на диск. [Экран справки](images/help.png).

**F2 → Appearance** меняет палитру интерфейса независимо от бумаги книги (**Space t**). Terminal использует стандартный фон, текст и цвета ANSI; изменения настроек самого терминала видны без перезапуска. В непрозрачном режиме цвет бумаги берётся из ответа терминала (OSC 11).

Если ответ недоступен, используется ANSI black или заданный вами `opaque_color`. Ни pywal, ни matugen не требуются. Встроены Gruvbox, Catppuccin, Nord, Dracula, Tokyo Night, Solarized, One Dark, Kanagawa, Rosé Pine и Everforest.

Выбранная именная палитра сохраняется; прозрачность показывает фон окна. Здесь же настраиваются углы рамок и обложек: Rounded, Square или None.

[Настройки интерфейса](images/settings.png)

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
# PDF/DjVu, comics and image-only EPUB: original, inverted, terminal, or a reading
# palette ID such as sepia-light or ember-dark. F2 and Space t save the same choice.
scan_theme = "original"
interface_theme = "catppuccin-mocha"
# Optional override. By default, solid Terminal paper follows the active terminal background.
# opaque_color = "#1f2125"
zen = false
# Show a dim page count and percentage in zen mode. Toggle with Space p.
zen_progress = false
# PDF/DjVu zoom in percent. In scroll mode 100 fits the reading pane.
scan_zoom = 100
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

Для менеджера паролей используйте `password_command`. Массив запускает команду с указанными аргументами. Строка выполняется через `sh -c`. Команда должна успешно завершиться и выдать пароль первой строкой. Для хранения открытым текстом есть поле `password`.

Если оба поля пусты, пароль вводится при входе. Приоритет имеет введённый пароль, затем команда, затем пароль из конфига. Сохранённые данные привязаны к адресу сервера и имени пользователя. Токены находятся в памяти, обычный выход завершает серверную сессию.

Читалка создаёт файл с правами `0600`. При обновлении она переносит старые настройки, подключение и привязки из JSON в TOML и сохраняет оригиналы в `$XDG_STATE_HOME/botui/config-backup`. Изменения через интерфейс сохраняют комментарии и посторонние ключи.

Доступны палитра терминала и схемы чтения BookOrbit. **Space l f** задаёт фон ячеек. Прозрачность всего окна зависит от терминала и композитора.

| Данные | Путь по умолчанию |
| --- | --- |
| Конфигурация | `~/.config/botui/config.toml` |
| Позиции, локальные закладки, заметки и история | `~/.local/state/botui/` |
| Скачанные книги и экспорт заметок | `~/.local/share/botui/downloads/`, `~/.local/share/botui/exports/` |

Каталоги можно переопределить переменными XDG. Локальные позиции привязаны к содержимому файла. Переименование и перенос книги сохраняют место чтения.

PDF/DjVu: `Space t` открывает компактное окно **Page theme** поверх видимой страницы. В списке **Original**, **Inverted**, Terminal и темы чтения. ↑/↓ сразу показывают предпросмотр; ←/→ выбирают светлый/тёмный вариант. Enter или клик применяет выбор. Esc или клик вне окна отменяет предпросмотр и возвращает прежнее оформление без сохранения. **F2 → Reader → PDF / image theme** и `Space t` сохраняют один параметр `scan_theme`: `original`, `inverted`, `terminal` или ID палитры, например `sage-dark`. Он применяется при повторном открытии и к следующим PDF/DjVu, комиксам и EPUB из изображений. Старые флаги цвета в позиции отдельной книги не переопределяют конфиг. Обычная тема чтения `theme` независима; отмена предпросмотра ничего не сохраняет. Нейтральные бумага и текст принимают цвета чтения; насыщенные иллюстрации сохраняют цвет. `Space l f` задаёт прозрачность как в EPUB/FB2. F2 и полки используют `interface_theme`.

### Действия по правому клику

Правый клик по слову или внутри выделения открывает маленькое меню возле указателя: **Copy, Highlight, Search, Translate, Define, Add note**. Книга остаётся видна за меню. Клик внутри выделения сохраняет весь фрагмент; клик по другому слову выбирает это слово. Search открывает поиск по текущей книге с выбранным текстом (в PDF сразу запускает поиск по текстовому слою). Esc или клик вне меню закрывает его; выбрать действие можно мышью либо ↑/↓ и Enter. Действия одинаковы для EPUB, FB2 и PDF со встроенным текстовым слоем; скан без текста не даёт действий над словом. Translate и Define используют подключения, выбранные в F2 → Dictionaries.


### Комиксы CBZ и CBR

CBZ (ZIP) и CBR (RAR) открываются встроенными декодерами: устанавливать распаковщики не нужно. Поддерживаются страницы JPEG, PNG и WebP, естественный порядок имён (1, 2, 10), обложка по первой странице, оглавление, масштаб, поворот, развороты, прокрутка, закладки и заметки на страницу. Позиция сохраняется после закрытия. Текстового поиска, словарей и выделения слов у комиксов нет — распознавание изображений не выполняется. Запароленные и многотомные CBR пока не поддерживаются; предел отдельной страницы — 16 МиБ. Сжатые RAR4/RAR5 и solid-варианты проверяются тестовыми архивами.

### Дополнительные палитры

В **F2 → Appearance → Interface theme** доступны новые тёмные **Graphite**, **Midnight**, **Pine** и **Plum** (`graphite`, `midnight`, `pine`, `plum`). Для чтения добавлены **Parchment**, **Sage**, **Dusk** и **Slate**, каждая со светлым и тёмным вариантом: например `parchment-light`, `sage-dark`, `dusk-light`, `slate-dark`. Они доступны в выборе обычной темы чтения и **PDF / image theme**.

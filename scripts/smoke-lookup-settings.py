#!/usr/bin/env python3
"""Exercise dictionary setup by mouse and keyboard with an isolated config."""
import importlib.util
import os
import tempfile
import tomllib
from pathlib import Path
spec = importlib.util.spec_from_file_location('shelf', Path(__file__).with_name('smoke-shelf.py'))
s = importlib.util.module_from_spec(spec)
spec.loader.exec_module(s)
binary = Path(os.environ.get('BOTUI_BINARY', s.ROOT / 'target/debug/botui'))
with tempfile.TemporaryDirectory(prefix='botui-lookup-settings-') as tmp:
    root = Path(tmp)
    config = root / 'config/botui/config.toml'
    config.parent.mkdir(parents=True)
    config.write_text('''leader_delay_ms = 150
[lookup]
dictionary = "English dictionary"
translation = "Google Translate"
[[lookup.profiles]]
name = "English dictionary"
provider = "free-dictionary"
[[lookup.profiles]]
name = "Google Translate"
purpose = "translation"
provider = "google-web"
[[lookup.profiles]]
name = "Local definitions"
provider = "command"
command = ["printf", "Definition: %s", "{text}"]
[[lookup.profiles]]
name = "Private DeepL"
purpose = "translation"
provider = "deepl"
api_key = "private-fixture-key"
[[opds]]
name = "Untouched catalog"
url = "https://example.org/opds"
''')
    book = root / 'sample.fb2'
    book.write_text('<FictionBook><body><section><p>Hello book.</p></section></body></FictionBook>')
    t = s.Terminal(root, binary, args=[str(book)])
    def click(label):
        frame = t.see(label)
        t.click(label, frame)
    def replace(value):
        t.keys('C-u')
        t.run('send-keys', '-l', '-t', 'shelf', value)
        t.keys('Enter')
    def data(): return tomllib.loads(config.read_text())
    try:
        t.see('Hello book.')
        t.keys('F2', 'Tab', 'Tab', 'Tab', 'Tab')
        frame = t.see('Word definitions')
        assert 'Space menu delay' not in frame and 'Connections' not in frame
        click('Word definitions');click('English dictionary')
        frame = t.see('api.dictionaryapi.dev/api/v2/entries/en')
        assert 'English word definitions' in frame and 'Does not translate text.' in frame, frame
        for unwanted in ['not set', 'Model:', 'Program:', 'API key:', 'password manager']:
            assert unwanted not in frame, unwanted
        click('Name: English dictionary');replace('Renamed dictionary')
        t.keys('Escape');t.see('Unsaved changes');click('Discard changes')
        assert data()['lookup']['dictionary'] == 'English dictionary'
        click('English dictionary');click('Name: English dictionary');replace('My dictionary')
        click('Save changes & back')
        t.see('My dictionary')
        assert data()['lookup']['dictionary'] == 'My dictionary'
        t.keys('Escape');click('Translation');click('Google Translate')
        frame=t.see('translate.googleapis.com/translate_a/single')
        assert 'API key:' not in frame and 'Model:' not in frame
        t.keys('Escape')
        click('Private DeepL');frame=t.see('Saved (hidden)')
        assert 'private-fixture-key' not in frame and 'Model:' not in frame and 'Program:' not in frame
        t.keys('Escape')
        t.keys('Escape');click('Word definitions');click('Local definitions');click('Program: printf')
        t.see('Argument 2: {text}')
        click('Argument 1: Definition: %s');replace('Meaning: %s')
        click('Apply command');click('Test connection with')
        t.see('Meaning: hello');t.keys('Escape')
        click('Use by default for dictionary: No');t.see('Use by default for dictionary: Yes');click('Save changes & back');t.see('Local definitions [default]')
        assert data()['lookup']['dictionary']=='Local definitions'
        assert data()['lookup']['profiles'][2]['command']==['printf','Meaning: %s','{text}']
        # Add and delete a configured service; back navigation preserves the list.
        click('+ Add dictionary')
        t.see('choose service');t.keys('Enter')
        t.see('Free Dictionary');click('Save changes & back');t.see('+ Add dictionary')
        assert len(data()['lookup']['profiles'])==5
        click('Free Dictionary · English definitions ·');click('Delete connection')
        t.keys('Down','Enter')
        t.see('Local definitions')
        assert len(data()['lookup']['profiles'])==4
        # The unrelated delay now lives under Reader.
        t.keys('Escape');t.see('Word definitions · 2 connections');t.keys('Left','Left','Left');t.see('Reading theme');t.keys('End')
        click('Space menu delay');replace('75');t.see('75 ms')
        assert data()['leader_delay_ms']==75
        assert data()['lookup']['profiles'][3]['api_key']=='private-fixture-key'
        assert data()['opds'][0]['name']=='Untouched catalog'
        print('PASS dictionary editor: relevant fields, defaults, real addresses, secret masking, rename/discard/save, local arguments/test, add/delete, Reader delay and config preservation')
    finally:
        t.run('kill-server', required=False)

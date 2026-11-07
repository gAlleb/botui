// Compare the Rust mapping with an independent DOM/Foliate installation; no server writes.
import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import { createRequire } from 'node:module'
import { resolve, basename } from 'node:path'
import { pathToFileURL } from 'node:url'
import { execFileSync } from 'node:child_process'
const [foliate, jsdomRoot, ...files] = process.argv.slice(2)
const { JSDOM } = createRequire(resolve(jsdomRoot,'package.json'))('jsdom')
const {window}=new JSDOM('')
for(const n of ['window','document','DOMParser','XMLSerializer','Node','NodeFilter','Range','HTMLElement','customElements'])globalThis[n]=window[n]
const load=name=>import(pathToFileURL(resolve(foliate,name)))
const {makeBook}=await load('view.js'), {makeFB2}=await load('fb2.js')
const view=customElements.get('foliate-view').prototype, exe=resolve('target/debug/examples/cfi-probe')
for(const name of files){
    const file=new File([readFileSync(name)],basename(name));const book=await (name.endsWith('.fb2')?makeFB2:makeBook)(file)
    const samples=JSON.parse(execFileSync(exe,[name]));assert.ok(samples.length>0)
    const fromWeb=[]
    for(const s of samples){
        const loc=view.resolveCFI.call({book},s.cfi)
        const doc=await book.sections[loc.index].createDocument()
        const range=loc.anchor(doc);const str=range.startContainer.textContent.slice(range.startOffset)
        const expected=s.fragment.trimEnd()
        assert.ok(str.replace(/\s+/gu,' ').startsWith(expected.slice(0,Math.min(expected.length,str.length))) && str.length>0,'Различается фрагмент '+s.cfi)
        fromWeb.push(view.getCFI.call({book},loc.index,range))
    }
    const returned=JSON.parse(execFileSync(exe,[name,'--resolve'],{input:JSON.stringify(fromWeb)}))
    assert.deepEqual(returned,samples.map(s=>[s.block,s.byte]))
    console.log(basename(name).split('.').at(-1),samples.length,'точных переходов Rust → Foliate → Rust')
}

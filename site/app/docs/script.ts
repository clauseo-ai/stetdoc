/**
 * The docs pages' one inline script (besides COPY_SCRIPT), kept by
 * strip-runtime through data-keep. Everything it does is an enhancement:
 * the sheets, the Open in menu and every link work without it.
 *
 * - Search: Pagefind loads by dynamic import() on the first open or the first
 *   shortcut, never before. "/" and Cmd K or Ctrl K open it; Enter in the
 *   field opens the first result, the arrows move focus through the result
 *   links (typing goes back to the field), Esc closes.
 * - Focus goes back to whatever opened a sheet when it closes.
 * - A link chosen inside a sheet closes the sheet, so an in-page jump isn't
 *   left under it.
 * - Copy page: the page's markdown from its <template>, or fetched from the
 *   twin for the largest pages, through a ClipboardItem promise so Safari
 *   keeps the click's permission across the fetch.
 * - The on-page TOC marks the section in view (a scroll listener, throttled
 *   to one check per 60 ms).
 *
 * No backticks inside: the bundler's minifier mangles a backtick string
 * folded into a template literal.
 */
import { DOCS_UI } from './strings';

const L = JSON.stringify({
	loading: DOCS_UI.searchLoading,
	failed: DOCS_UI.searchFailed,
	one: DOCS_UI.searchCount(1),
	many: DOCS_UI.searchCount(2).replace('2', '#'),
	empty: DOCS_UI.searchEmpty('#'),
	copied: DOCS_UI.copied,
	copyFailed: DOCS_UI.copyFailed,
});

export const DOCS_SCRIPT = `(function(){
var d=document,L=${L},S=d.getElementById('search'),I=d.getElementById('search-input'),R=d.getElementById('search-results'),T=d.getElementById('search-status'),pf,q0='',items=[];
function esc(s){return String(s).replace(/[&<>"]/g,function(c){return '&#'+c.charCodeAt(0)+';'})}
function clean(u){return u.replace(/\\.html(?=#|$)/,'')}
function load(){if(!pf){T.textContent=L.loading;pf=import('/pagefind/pagefind.js').then(function(m){return Promise.resolve(m.init&&m.init()).then(function(){T.textContent='';return m})});pf.catch(function(){T.textContent=L.failed;pf=null})}return pf}
function go(i){var n=items.length;if(i<0||!n){I.focus();return}items[Math.min(i,n-1)].firstChild.focus()}
function render(ds,q){R.innerHTML=ds.map(function(x,i){var s=x.sub_results&&x.sub_results[0],h=s&&s.title&&s.title!==x.meta.title&&s.url.indexOf('#')>0?s:null,u=clean(h?h.url:x.url);
return '<li><a href="'+esc(u)+'"><span class="sr-title">'+esc(x.meta.title||u)+'</span><span class="sr-where">'+esc([x.meta.section,h&&h.title].filter(Boolean).join(' / '))+'</span><span class="sr-ex">'+(h?h.excerpt:x.excerpt)+'</span></a></li>'}).join('');
items=R.children;
T.textContent=ds.length?(ds.length===1?L.one:L.many.replace('#',ds.length)):L.empty.replace('#',q)}
function run(){var q=I.value.trim();if(q===q0)return;q0=q;if(!q){R.innerHTML='';items=[];T.textContent='';return}
load().then(function(m){return m.debouncedSearch(q,{},120)}).then(function(r){if(!r||q!==I.value.trim())return;return Promise.all(r.results.slice(0,8).map(function(x){return x.data()}))}).then(function(ds){if(ds)render(ds,q)}).catch(function(){})}
if(S){I.addEventListener('input',run);
I.addEventListener('keydown',function(e){if(e.key==='ArrowDown'&&items.length){e.preventDefault();go(0)}else if(e.key==='Enter'&&items.length){e.preventDefault();var a=items[0].firstChild;S.hidePopover();location.href=a.href}else if(e.key==='Escape'){e.preventDefault();S.hidePopover()}});
R.addEventListener('keydown',function(e){var li=e.target.closest('li'),i=[].indexOf.call(items,li);if(e.key==='ArrowDown'){e.preventDefault();go(i+1)}else if(e.key==='ArrowUp'){e.preventDefault();go(i-1)}else if(e.key.length===1&&!e.metaKey&&!e.ctrlKey){I.focus()}});
S.addEventListener('beforetoggle',function(e){if(e.newState==='open')load()});
S.addEventListener('toggle',function(e){if(e.newState==='open'){I.focus();I.select()}});
d.addEventListener('keydown',function(e){var t=e.target,typing=t.closest&&t.closest('input,textarea,select,[contenteditable]');
if((e.key==='k'||e.key==='K')&&(e.metaKey||e.ctrlKey)&&!e.altKey&&!e.shiftKey){e.preventDefault();S.matches(':popover-open')?S.hidePopover():S.showPopover()}
else if(e.key==='/'&&!typing&&!e.metaKey&&!e.ctrlKey&&!e.altKey&&!S.matches(':popover-open')){e.preventDefault();S.showPopover()}})}
[].forEach.call(d.querySelectorAll('[popover]'),function(p){var o;
p.addEventListener('beforetoggle',function(e){if(e.newState==='open')o=e.source||d.activeElement});
p.addEventListener('toggle',function(e){var a=d.activeElement;if(e.newState==='closed'&&o&&o.focus&&(!a||a===d.body||p.contains(a)))o.focus({preventScroll:true});
if(e.newState==='open'){var c=p.querySelector('[aria-current=page]'),b=c&&c.closest('.sheet-body');if(b)b.scrollTop=c.offsetTop-b.clientHeight/2}})});
d.addEventListener('click',function(e){var a=e.target.closest&&e.target.closest('a[href]'),p=a&&a.closest('[popover]');if(p&&p.matches(':popover-open'))p.hidePopover();
var b=e.target.closest&&e.target.closest('[data-copy-page]');if(!b)return;var s=b.nextElementSibling,tp=d.getElementById('page-md'),C=navigator.clipboard,ok=C&&window.isSecureContext;
function done(){b.setAttribute('data-copied','');s.textContent=L.copied;clearTimeout(b._t);b._t=setTimeout(function(){b.removeAttribute('data-copied');s.textContent=''},1500)}
function fail(){s.textContent=L.copyFailed}
function legacy(t){var x=d.createElement('textarea');x.value=t;x.setAttribute('readonly','');x.style.position='fixed';x.style.opacity='0';d.body.appendChild(x);x.select();var y=false;try{y=d.execCommand('copy')}catch(z){}x.remove();y?done():fail()}
if(tp){var t=tp.content.textContent;ok?C.writeText(t).then(done,function(){legacy(t)}):legacy(t);return}
var f=fetch(b.getAttribute('data-copy-page')).then(function(r){if(!r.ok)throw 0;return r.text()});
if(ok&&C.write&&window.ClipboardItem)C.write([new ClipboardItem({'text/plain':f.then(function(t){return new Blob([t],{type:'text/plain'})})})]).then(done,function(){f.then(legacy,fail)});
else f.then(function(t){return ok?C.writeText(t):Promise.reject()}).then(done,function(){f.then(legacy,fail)})});
var links=d.querySelectorAll('.toc-list a'),hs=[],seen={},raf=0;
[].forEach.call(links,function(a){var id=decodeURIComponent(a.hash.slice(1));if(!seen[id]){seen[id]=1;var h=d.getElementById(id);if(h)hs.push(h)}});
function spy(){raf=0;var cur='';for(var i=0;i<hs.length;i++){if(hs[i].getBoundingClientRect().top<120)cur=hs[i].id;else break}
[].forEach.call(links,function(a){decodeURIComponent(a.hash.slice(1))===cur?a.setAttribute('aria-current','location'):a.removeAttribute('aria-current')})}
if(hs.length){addEventListener('scroll',function(){if(!raf)raf=setTimeout(spy,60)},{passive:true});spy()}
var side=d.querySelector('.side'),cur=side&&side.querySelector('[aria-current=page]');if(cur&&cur.offsetTop+cur.offsetHeight>side.clientHeight*0.8){var cs=getComputedStyle(side),pt=parseFloat(cs.paddingTop),h=side.clientHeight-parseFloat(cs.paddingBottom),want=Math.min(cur.offsetTop-h/3,side.scrollHeight-side.clientHeight),top=0;
[].forEach.call(side.querySelectorAll('.nav-sec'),function(s){var t=s.offsetTop-pt;if(t<=want&&cur.offsetTop+cur.offsetHeight-t<=h)top=t});if(cur.offsetTop+cur.offsetHeight-top>h)top=want;side.scrollTop=top}
})()`;

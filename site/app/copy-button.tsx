/**
 * The install command's Copy button, with no client component: a plain
 * button, and one small inline script (COPY_SCRIPT, rendered once by the
 * page) that copies, says "Copied" for 1.5 s and announces it through the
 * status span beside the button. It also marks every key that copies the
 * same text data-chosen for good: the suggested install's keys are court
 * yellow until one of them is used. Both labels share one grid cell, so the
 * button keeps its width. Where the Clipboard API is missing (plain http on
 * a LAN address) it falls back to a selected textarea, and if that fails
 * too it selects the command on the page and says so.
 */
import { UI } from '../content/copy';

export function CopyButton({
	text,
	label = UI.copyLabel,
}: {
	text: string;
	label?: string;
}) {
	return (
		<>
			<button
				type='button'
				className='copy'
				aria-label={label}
				data-copy={text}>
				<span className='copy-idle'>{UI.copy}</span>
				<span className='copy-done'>{UI.copied}</span>
			</button>
			<span
				className='sr-only'
				role='status'
			/>
		</>
	);
}

export const COPY_SCRIPT = `document.addEventListener('click',function(e){
var b=e.target.closest&&e.target.closest('[data-copy]');if(!b)return;
var t=b.getAttribute('data-copy'),s=b.nextElementSibling;
function done(){b.dataset.copied='';
document.querySelectorAll('[data-copy]').forEach(function(x){x.dataset.copy==t&&(x.dataset.chosen='')});s.textContent=${JSON.stringify(UI.copied)};
clearTimeout(b._t);b._t=setTimeout(function(){delete b.dataset.copied;s.textContent=''},1500)}
function legacy(){var a=document.createElement('textarea');a.value=t;a.setAttribute('readonly','');
a.style.position='fixed';a.style.opacity='0';document.body.appendChild(a);a.select();
var ok=false;try{ok=document.execCommand('copy')}catch(x){}a.remove();if(ok)done();else pick()}
function pick(){var c=(b.closest('.install,.for-agents')||b.parentNode).querySelector('code'),r=document.createRange();r.selectNodeContents(c);var g=getSelection();g.removeAllRanges();g.addRange(r);s.textContent=${JSON.stringify(UI.copySelected)}}
if(navigator.clipboard&&window.isSecureContext)navigator.clipboard.writeText(t).then(done,legacy);else legacy()})`;

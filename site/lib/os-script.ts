/**
 * The launch page's second inline script, rendered before the hero so it
 * runs before the first paint. It reads the visitor's system and, for macOS,
 * Linux or Windows, sets data-os on <html>: CSS then shows that system's
 * install in the hero, the "Suggested" chip and that channel's picker tag.
 * When the page has loaded it checks that channel in the picker and moves its
 * tab first. A phone, a tablet, ChromeOS, an unknown system or script off
 * leaves data-os unset, and the page shows all three installs, labelled.
 * strip-runtime.ts keeps it (data-keep) and holds it to 500 bytes.
 */
import { INSTALLS } from '../content/copy';

const CHANNEL = Object.fromEntries(INSTALLS.map((i) => [i.os, i.channel]));

/** iPadOS says Macintosh, so a Mac with touch points is a tablet */
export const OS_SCRIPT = `(function(n,D){var d=n.userAgentData,p=(d&&d.platform||n.platform||'')+n.userAgent,o=/iP(hone|ad|od)|Android|CrOS/i.test(p)||/Mac/i.test(p)&&n.maxTouchPoints>1?'':/Win/.test(p)?'windows':/Mac/i.test(p)?'mac':/Linux|X11/.test(p)?'linux':'';if(o){D.documentElement.dataset.os=o;D.addEventListener('DOMContentLoaded',function(){var i=D.getElementById('ch-'+${JSON.stringify(CHANNEL)}[o]);if(i){i.checked=true;i.parentNode.prepend(i,i.nextElementSibling)}})}})(navigator,document)`;

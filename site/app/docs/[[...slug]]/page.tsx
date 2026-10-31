import type { Metadata } from 'next';
import { docJsonLd, docMetadata, docParams, getDoc, jsonLdText } from '../../../lib/docs';
import { Breadcrumbs, EndMark, PageActions, PageFoot, Pager } from '../page-parts';
import { DocsShell } from '../shell';

/* one optional catch-all serves /docs and every page under it, all
   prerendered; an unknown path is the site's 404 */
export const dynamicParams = false;
// static export wants every param present: the overview is slug []
export const generateStaticParams = () => docParams().map((p) => ({ slug: p.slug ?? [] }));

type Props = { params: Promise<{ slug?: string[] }> };
const slugOf = async (params: Props['params']) => ((await params).slug ?? []).join('/');

export async function generateMetadata({ params }: Props): Promise<Metadata> {
	return docMetadata(getDoc(await slugOf(params)));
}

export default async function DocPage({ params }: Props) {
	const doc = getDoc(await slugOf(params));
	const json = jsonLdText(docJsonLd(doc));
	return (
		<DocsShell
			current={doc.path}
			toc={doc.toc}>
			<script
				type='application/ld+json'
				dangerouslySetInnerHTML={{ __html: json }}
			/>
			<article
				className={doc.slug ? 'doc' : 'doc doc-overview'}
				data-pagefind-body=''
				data-pagefind-meta={`section:${doc.sectionTitle}`}>
				<header className='dochead'>
					<div
						className='doc-crumbs'
						data-pagefind-ignore=''>
						{/* the overview has no trail; on a phone its section names the row
						    the page keys sit on, as the trail does on every other page */}
						{doc.slug ? <Breadcrumbs trail={doc.breadcrumbs} /> : <p className='doc-eyebrow'>{doc.sectionTitle}</p>}
					</div>
					<h1>{doc.title}</h1>
					<div
						className='doc-tools'
						data-pagefind-ignore=''>
						<PageActions doc={doc} />
					</div>
				</header>
				<div className='prose'>{doc.body}</div>
			</article>
			<PageFoot doc={doc} />
			<EndMark />
			<Pager
				prev={doc.prev}
				next={doc.next}
			/>
		</DocsShell>
	);
}

# Corpus sources

One entry per source. The repository commits no third-party document. `cargo xtask corpus fetch` downloads each file from its source at the pinned commit, checks its SHA-256 against the manifest and caches it. `vendorable` records whether the source licence would allow vendoring after a licence review (no document is vendored now). `generated-*` sources are our own output: `generated-edge` and `generated-hostile` are built in tests by the testkit fixture builder, and nothing is stored. Superdoc (AGPL-3.0) and jubarte-redlines (AGPL-3.0) were inspected and their files are not taken. Xceed DocX (non-commercial community licence) and SheetJS js-word (copies of other projects' files) were looked at and left out.

Licence caveat: a repository licence covers the repository author's own files. Test suites often contain third-party documents whose own licence is not stated. `vendorable` is `yes` only where the source is a permissive open-source test suite and the documents look like test fixtures; where a source contains documents with a known different origin (NVCA, EU texts, a safety data sheet) it is `no`. Because nothing is vendored, this matters only if a later review decides to commit documents.

| source | URL | pinned commit | SPDX | how obtained | vendorable | unique docs | in curated | what we take |
|---|---|---|---|---|---|---:|---:|---|
| open-xml-sdk | https://github.com/dotnet/Open-XML-SDK | `431ab05cf160248cc3885a4a766026d4f8243792` | MIT | fetch | yes | 456 | 18 | Test assets (TestFiles, TestDataStorage): Word-authored files from Word 2007 to 2016 including the O15 comment-extension conformance set, content controls, charts, SmartArt, tracked changes, and deliberately invalid packages. The richest single source of real Word output. |
| open-xml-powertools | https://github.com/OfficeDev/Open-Xml-PowerTools | `5881422a881f6ccefce2b9801b5dc6a753670d6e` | MIT | fetch | yes | 713 | 9 | TestFiles for RevisionProcessor, DocumentBuilder, WmlComparer and HtmlConverter (RP*, WC*, DB*, HC* series). Minimal Word-authored documents for each revision shape, plus before/after pairs for compare. |
| poi | https://github.com/apache/poi | `b9de5f42fd8a7134cc221da68a62261e1087d056` | Apache-2.0 | fetch | yes | 186 | 14 | test-data/document: Word 2007 to 2010 files, LibreOffice and Apache POI output, fuzzer minimised cases (clusterfuzz-testcase-*), password-protected OLE containers, ZIP oddities (trailing bytes, unicode paths). |
| python-docx | https://github.com/python-openxml/python-docx | `e45454602b53e8e572b179ccf1c91093ec9f4ed7` | MIT | fetch | yes | 45 | 1 | Steve Canny's feature-test documents and the default template that Harvey LAB inherits. Documents with comments, styles, sections, tables. |
| docx4j | https://github.com/plutext/docx4j | `0e8e7633ef46012e0d3603339728b83ed21d5045` | Apache-2.0 | fetch | yes | 142 | 6 | Test resources and developer docs: docx4j output, Word 2010 features (mc:AlternateContent, glow effects, OpenDoPE), metafile docs. |
| mammoth-js | https://github.com/mwilliamson/mammoth.js | `791a189230977143cde64dbf9f1d24dce96ac8a1` | BSD-2-Clause | fetch | yes | 17 | 1 | Word-authored documents for comments, footnotes, endnotes, tables, hyperlinks, text boxes, UTF-8 BOM, embedded style maps. |
| pandoc | https://github.com/jgm/pandoc | `37b5a8a9e96ee5cfba772cfd1349e05274e49820` | GPL-2.0-or-later | fetch | no | 136 | 4 | test/docx: Word-authored and Pandoc-produced documents (track changes, comments, lists, tables). Fetch-only because the test data is GPL-2.0-or-later. |
| libreoffice-core | https://github.com/LibreOffice/core | `dfbde789eeef41f8aee33de12b7b674a49760470` | MPL-2.0 | fetch | no | 2263 | 93 | sw/qa, oox/qa, writerfilter/qa, chart2/qa, package/qa data: about 2,300 documents from Word, LibreOffice, Google Docs (customXml gDocsCustomXmlDataStorage), WPS, plus fuzzer cases (ofz*), ZIP structure tests (overlap, two-zips, inner-gap, zip64-eocd, dd-deflated, casing). Fetch-only: MPL-2.0 files are never committed. |
| tika | https://github.com/apache/tika | `de6af231dcb721856b719966bc010f909358319c` | Apache-2.0 | fetch | yes | 62 | 7 | tika-parsers test documents: Word-authored and edge cases (embedded objects, odd encodings). |
| docxtemplater | https://github.com/open-xml-templating/docxtemplater | `0894df85bdfa3157f6a6e55b74279fc57a9f0d4c` | MIT | fetch | yes | 174 | 5 | examples/: Word, LibreOffice and WPS authored templates and expected outputs with loops, images, section breaks, Hebrew and RTL. |
| docxjs | https://github.com/VolodymyrBaydalka/docxjs | `c533f383069008d4a1e05c6171ef8777fd078385` | Apache-2.0 | fetch | yes | 15 | 1 | tests/render-test: documents for a browser renderer (tables, lists, text boxes, fields), LibreOffice and Word authored. |
| unstructured | https://github.com/Unstructured-IO/unstructured | `f4105db674e4bf6c98ddb8fe6a5e32df0d7237b4` | Apache-2.0 | fetch | yes | 21 | 1 | example-docs: ordinary business documents and chat exports (Teams) from several producers. |
| docling | https://github.com/docling-project/docling | `b91e6b22b3fac9c15f3cb78b8db573c7c54f4d64` | MIT | fetch | yes | 35 | 2 | tests/data/docx: documents with lists, tables, equations, headers and footers. |
| markitdown | https://github.com/microsoft/markitdown | `4cc9fa17653d695d64fb9eee5b33d4de55ff84e8` | MIT | fetch | yes | 10 | 1 | packages/markitdown/tests/test_files: a handful of Word documents. |
| docx-rs | https://github.com/bokuweb/docx-rs | `4ff72dc8ba5f33ca40c121f30664d12e4fe6cdb2` | MIT | fetch | yes | 73 | 1 | Rust docx-rs fixtures: output of the docx-rs library (Un-named creator), including the zip-rs archive comment. |
| docx-js | https://github.com/dolanmiu/docx | `bd5ab7ace6075349b6b260eee5a8f2371f603202` | MIT | fetch | yes | 138 | 5 | dolanmiu/docx demo output and test assets: the docx.js producer (JSZip directory entries, Un-named creator). |
| officeimo | https://github.com/EvotecIT/OfficeIMO | `3472faec6b9eb4bdec147204d3d7319e4fb18f91` | MIT | fetch | yes | 113 | 5 | OfficeIMO test assets and examples: Word-authored templates and OfficeIMO output (Open XML SDK based), charts, symbol bullets, password and recommendation examples. |
| aspose-words-net | https://github.com/aspose-words/Aspose.Words-for-.NET | `1b48badc2a1a264f3c0294a64ec5c6ec1ea1359e` | MIT | fetch | yes | 320 | 15 | Examples/Data: Word-authored sample documents from the Aspose.Words examples repository (bookmarks, TOC fields, headers, corrupted document). |
| npoi | https://github.com/nissl-lab/npoi | `832eb28f142d54fb8a87b65fa61b5916a763b528` | Apache-2.0 | fetch | yes | 9 | 1 | NPOI test-data for XWPF: Word 2007+ files from the Apache POI lineage. |
| docx-templates | https://github.com/guigrpa/docx-templates | `54c2e80a090d0219503df1f26af91228b9880d77` | MIT | fetch | yes | 112 | 1 | guigrpa/docx-templates fixtures: Word-authored templates with commands in fields and content controls. |
| open-agreements | https://github.com/open-agreements/open-agreements | `454c20d973dc280e5dc82a3b40ae55b996051f9c` | Apache-2.0 (code); per-template licences below | fetch | partly (197 of 201) | 201 | 10 | Legal templates: Common Paper (CC BY 4.0), Bonterms (CC0 1.0), OpenAgreements originals (CC BY 4.0 and CC0), Y Combinator SAFE (CC BY-ND 4.0, fetch-only), NVCA documents (not redistributable, never taken). Per-template licence from metadata.yaml. Produced by the safe-docx tool. |
| clippit | https://github.com/sergey-tihon/Clippit | `88195321a87a450a26108d6a09ecc8aff9dadd6d` | MIT | fetch | yes | 70 | 1 | Maintained fork of Open-Xml-PowerTools. Almost all files duplicate the PowerTools set (deduplicated by SHA-256). |
| poi-tl | https://github.com/Sayi/poi-tl | `b5005446e2af099537327c5929cb17286e5e085f` | Apache-2.0 | fetch | yes | 116 | 5 | Word templates for the poi-tl template engine (Word-authored, Apache-2.0). |
| docsharp | https://github.com/manfromarce/DocSharp | `7c996f8f6934139d4a4a698963baeb59fd8d60df` | MIT | fetch | yes | 51 | 4 | DocSharp tests: documents from several .NET converters. |
| docx-mcp | https://github.com/SecurityRonin/docx-mcp | `9c0c0b7694d8123e82fe6b7c480899890dca0695` | MIT | fetch | yes | 4 | 0 | docx-mcp fixtures: Pandoc-produced tracked-change documents and others. |
| docx-redline-js | https://github.com/AnsonLai/docx-redline-js | `683c30a318d3c980ff59c15ae0c619d82b770530` | MIT | fetch | yes | 63 | 1 | docx-redline-js fixtures: tracked-change scenarios. |
| document-change-benchmark | https://github.com/SybilGambleyyu/document-change-benchmark | `a537bea556ce492a435946fb8f1f41ef0e9930bc` | MIT | fetch | yes | 78 | 3 | Redline benchmark pairs (original and revised documents). |
| red-line-district | https://github.com/lawyer-not-liar/red-line-district | `0626b4cd806e65977f5f1b5efc726c0ed6c03797` | MIT | fetch | yes | 47 | 0 | Redline tool fixtures. |
| fixtures-ooxml | https://github.com/rcarmo/fixtures-ooxml | `0cf83c156c8e9433347e446e58c415db011b17be` | MIT | fetch | yes | 30 | 1 | Rui Carmo fixtures-ooxml: generated fixtures per feature (comments, content controls, and others). |
| ruby-docx | https://github.com/ruby-docx/docx | `e083d0b59d2ed55dc9ecbd3e40e0714167a1119c` | MIT | fetch | yes | 21 | 3 | ruby-docx spec fixtures: bookmarks, tables, styles. |
| word-to-markdown | https://github.com/benbalter/word-to-markdown | `69194c4566b9b4ca2f97d80094bec9445e15000b` | MIT | fetch | yes | 18 | 3 | Ben Balter word-to-markdown test documents. |
| html2openxml | https://github.com/onizet/html2openxml | `a887e140c5be32c7d30576775c8b570b03c8f58d` | MIT | fetch | yes | 6 | 0 | Html2OpenXml test documents. |
| amberg-docxtemplater | https://github.com/Amberg/DocxTemplater | `e94ac862aca276ef2e57d806b979f7fa34162705` | MIT | fetch | yes | 32 | 0 | Amberg/DocxTemplater .NET template fixtures. |
| docxbox | https://github.com/gyselroth/docxBox | `ce7c4db5ef0d99f697b07cd086967c2bdb649f33` | MIT | fetch | yes | 7 | 1 | gyselroth docxBox sample documents. |
| pyopenvba | https://github.com/WilliamSmithEdward/pyOpenVBA | `2b41b07ca817538be628cd4591d13a9b2c92c058` | MIT | fetch | yes | 14 | 0 | Macro-enabled documents (docm) with VBA projects. |
| betteroffice | https://github.com/openooxml/betteroffice | `17a722c58d2457a49b4601bc3ac63bd10587290c` | Apache-2.0 | fetch | yes | 97 | 3 | openooxml/betteroffice fixtures, including comment-range structure documents. |
| docassemble-assemblyline | https://github.com/SuffolkLITLab/docassemble-AssemblyLine | `ed08f23f57b934476cd65a12f05edca2ac518003` | MIT | fetch | yes | 9 | 2 | Suffolk LIT Lab court-form Word templates (MIT). |
| officecli | https://github.com/iOfficeAI/OfficeCLI | `d49beb48af7c7f50c4ec745269ac4b5b44c866dc` | Apache-2.0 | fetch | yes | 20 | 5 | iOfficeAI/OfficeCLI test documents. |
| officeagent-net | https://github.com/ilia-sokolov/OfficeAgent.NET | `80596e619478e306337bfd9bc70cdb854cf55c6a` | MIT | fetch | yes | 57 | 2 | OfficeAgent.NET test documents. |
| yumdocs | https://github.com/yumdocs/yumdocs | `25326df8a111379caf4ad58d9fd8cc04aca3e421` | MIT | fetch | yes | 13 | 0 | YumDocs templates. |
| docxplate | https://github.com/bobiverse/docxplate | `f80cf8d6eeed90525f9915bd25c31f17d4d19018` | MIT | fetch | yes | 12 | 1 | docxplate templates. |
| sablon | https://github.com/CommonPaper/sablon | `a301dd39048e4cf94c3f82f7302efc9e1f1f8f24` | MIT | fetch | yes | 18 | 1 | Common Paper sablon (Ruby docx templating) fixtures; Word-authored templates with conditionals and loops. |
| bonterms-cloud-terms | https://github.com/Bonterms/Cloud-Terms | `df13f512c3bd923cc09cc2dae918812f876ea6e9` | CC-BY-4.0 | fetch | yes | 2 | 0 | Bonterms Cloud Terms example cover page and playbook (CC BY 4.0). |
| bonterms-mutual-nda | https://github.com/Bonterms/Mutual-NDA | `1b6e3a67176d47bbb89633bd4ec2be2c32d6cacf` | CC-BY-4.0 | fetch | yes | 2 | 1 | Bonterms Mutual NDA example cover page and playbook (CC BY 4.0). |
| eu-ai-act | https://github.com/smucclaw/eu-ai-act | `ec26eabbc00c8eb6e17ebaa12a7b1bbbd25cfce3` | Apache-2.0 (code); document content LicenseRef-EU-institutions-or-unstated | fetch | no | 25 | 2 | EU AI Act drafts and consolidated versions as Word files (13 from the Commission, Word 2016/365). Content licence not stated per file, so fetch-only. |
| officedissector | https://github.com/grierforensics/officedissector | `2059a5ba08fa139362e3936578f99c4da9a9b55d` | GPL-3.0-or-later | fetch | no | 55 | 5 | Grier Forensics OfficeDissector test files (security research samples). GPL: fetch-only. |
| phpword | https://github.com/PHPOffice/PHPWord | `a40907eae98485f127da6b3d04a1cf628b54fa64` | LGPL-3.0-only | fetch | no | 25 | 2 | PHPWord samples. LGPL: fetch-only. |
| casualoffice-docs | https://github.com/CasualOffice/docs | `77e9530dc8ae9629399569fae4638214c39f48d3` | Apache-2.0 (code); document content unclear | fetch | no | 66 | 3 | CasualOffice docs samples of unclear origin: fetch-only. |
| docxodus | https://github.com/JSv4/Docxodus | `bcd1ee8cfb5b1855e1fee83231cd4c7b69ab177b` | MIT | fetch | yes | 51 | 1 | TestFiles: about 250 compare, revision and glossary documents derived from the PowerTools set, plus NVCA model documents (fetch-only because their licence is not stated). Local clone pinned at the recorded commit. |
| adeu | https://github.com/dealfluence/adeu | `965736d2d8ea32f42ed9a2a0a01b3c5a538f7f75` | MIT | fetch | yes | 24 | 2 | shared/cross_platform_tests, conformance and fixtures: agent redline scenarios including Word Online edited tables, nested revisions, counsel accept/reject. MIT. |
| docx-cli | https://github.com/kklimuk/docx-cli | `74672e4af3444d101399e28284ec19fc11de6821` | MIT | fetch | yes | 36 | 2 | tests/fixtures: tracked-change chains, comments, equations produced by docx-cli and checked against Word. |
| stemma | https://github.com/stemma-sh/stemma | `ad1e70deac0a828d5162ac3b3f2186c2bb0c075e` | Apache-2.0 OR MIT | fetch | yes | 647 | 3 | stemma-engine testdata and stemma-examples samples: redline, table and barrier cases. |
| scriptor | https://github.com/truespar/scriptor | `7d67ae99cfaf1cd342fb4949889fd681ee713c30` | Apache-2.0 OR MIT | fetch | yes | 2 | 1 | Two CRDT fixtures. |
| pyred | https://github.com/JSv4/Python-Redlines | `a84be23f0e3d333572a5b5a5d6db2205ae2cb5cf` | MIT | fetch | yes | 3 | 1 | Python-Redlines test fixtures: Docxodus compare inputs and expected redlines. |
| docxml | https://github.com/jwmurray/docxml | `97d47f786a5a133263f7d85c2eab058b69817186` | Apache-2.0 OR MIT | fetch | yes | 5 | 0 | jwmurray/docxml fixtures and template. |
| linch-docx-rs | https://github.com/laofahai/linch-docx-rs | `10a2803ae0e16b1957e57fe7f09d15fbb965972c` | MIT | fetch | yes | 1 | 0 | One fixture. |
| cat-agent-skills | https://github.com/microsoft/cat-agent-skills | `71194891766adb23a75287ca92abd0fb5f4b9acb` | MIT | fetch | no | 4 | 0 | Four sample documents in Microsoft agent-skill submissions (redlining, format conversion). Unclear per-file origin, so fetch-only. |
| harvey-labs | https://github.com/harveyai/harvey-labs | `465d1fcf270daa01b0d1d63e30e3da9876d7a268` | MIT | fetch | no | 41694 | 46 | Scale and legal content. 42,009 python-docx documents across 27 task categories. Fetched by pinned commit in CI and nightly, never vendored. A 60-per-category stratified sample was scanned in full. |
| generated-edge | local: stetdoc_testkit::fixtures (crates/stetdoc-testkit/src/fixtures) | `generated` | CC0-1.0 | generate | yes | 90 | 90 | Synthetic valid fixtures built byte by byte in memory by `stetdoc_testkit::fixtures`: one per ZIP, XML and OPC detail that the census found and no real document exercises, and minimal Word documents that each carry one feature. Tests build them, nothing is stored. Our own bytes, CC0. |
| generated-hostile | local: stetdoc_testkit::fixtures (crates/stetdoc-testkit/src/fixtures) | `generated` | CC0-1.0 | generate | yes | 86 | 86 | Synthetic hostile fixtures built in memory by the same module: bombs, traversal, entity attacks, truncation, lying headers, and packages with required parts missing. Each says what stetdoc must do with it and why. |

## Compare pairs

`pairs.tsv` lists the compare pairs (`docs/design/compare.md`, sections 1.2, 9 and 21) by the SHA-256 of the original and the revised document, each a document of `manifest/`, so it adds no source and no file. 779 pairs in four classes:

| class | source | pairs | curated | how a pair is found |
|---|---|---:|---:|---|
| lab | harvey-labs | 354 | 40 | two documents of one task directory of the `*redline*` and `*term-negotiation*` tasks that share at least half of the distinct paragraphs of 20 characters or more of the smaller, on the accepted text |
| oxpt | open-xml-powertools | 184 | 181 | every unordered pair inside one `TestFiles/WC` or `CA` family |
| dox | docxodus | 200 | 0 | every unordered pair inside one `HW`, `DB`, `LIR`, `RA` or `RPR` family |
| dcb | document-change-benchmark | 41 | 20 | the baseline and the candidate of one fixture directory |

`cargo xtask corpus fetch` fetches the documents of the curated pairs with the per-pull-request corpus, into a set of their own (`curatedpairs`, 275 documents), so the other corpus tests sample the same documents as before. `cargo xtask corpus fetch --pairs` fetches the documents of every pair into the `pairs` set (785 documents, 43 MB). A pair whose redline is a noise outlier of N9 has its reason in the last column.

## Per-template licences inside open-agreements

| directory | SPDX | vendorable after review |
|---|---|---|
| templates/bonterms-cc0-1.0/ | CC0-1.0 | yes |
| templates/common-paper-cc-by-4.0/ | CC-BY-4.0 | yes |
| templates/openagreements-cc-by-4.0/ | CC-BY-4.0 | yes |
| templates/openagreements-cc0-1.0/ | CC0-1.0 | yes |
| templates/yc-cc-by-nd-4.0/ | CC-BY-ND-4.0 | no |
| templates/nvca-free-non-redistributable/ | LicenseRef-NVCA-free-non-redistributable | no |

## Attribution required

Documents under CC-BY-4.0 (Common Paper, Bonterms, OpenAgreements) need attribution when they are redistributed. This repository and its releases redistribute none of them: the files are fetched from their sources into a local cache, `sources.tsv` records each source URL and pinned commit, and the `spdx` column of the manifest records each file's licence. If a document is ever committed or shipped, the same change must add its attribution: creator, source URL, licence, and a note of any change.

## Sources considered and rejected

| source | reason |
|---|---|
| superdoc (superdoc-dev/superdoc) | AGPL-3.0 |
| jubarte-redlines and neurotic_docx_bench (jandira-tech) | AGPL-3.0 |
| ONLYOFFICE-QA/ooxml_parser | AGPL-3.0 |
| xceedsoftware/DocX | Xceed Community License, non-commercial only |
| SheetJS/js-word | test_files are copies of Open-XML-SDK, POI, Pandoc and python-docx files, already taken from the originals |
| NVCA model documents | free to use, not redistributable (also in open-agreements `nvca-free-non-redistributable`); Docxodus has copies, fetch-only |
| Y Combinator SAFE | CC BY-ND 4.0, fetch-only if used |


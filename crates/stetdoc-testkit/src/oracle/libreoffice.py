# The LibreOffice oracle's runner: a Python macro that soffice runs in its own process (so it uses LibreOffice's bundled
# Python and UNO, and needs no separate Python interpreter). `oracle::libreoffice` copies it into a private profile as
# `user/Scripts/python/stetdoc_oracle.py` and starts
#
#   soffice --headless -env:UserInstallation=<profile> 'vnd.sun.star.script:stetdoc_oracle.py$run?language=Python&location=user'
#
# with three variables: STETDOC_LO_JOBS (a file with one .docx path per line, optionally followed by a tab and a variant that
# `ablated` makes of it first), STETDOC_LO_RESULTS (a file it appends one
# JSON line to for each document, in order) and STETDOC_LO_WORK (a scratch directory). It skips the documents that already
# have a line, so the caller can start it again after a crash or a hang. Each line is
#
#   {"path", "load": error or null, "pdf": error or null, "accept": lines or null, "reject": lines or null,
#    "notes_accept", "notes_reject", "left_accept", "left_reject", "error": error or null}
#
# where "accept" is the body's paragraphs (with the frames LibreOffice anchors to a paragraph, before it) after
# Edit > Track Changes > Accept All (the dispatch command .uno:AcceptAllTrackedChanges), "reject" after Reject All, one string a paragraph (a table's cells row by row, nested
# tables in place), "notes_*" the same for the footnotes and then the endnotes, and "left_*" how many tracked changes
# LibreOffice still has after accepting or rejecting all of them.

import json
import os
import re
import zipfile

import uno
from com.sun.star.beans import PropertyValue


def prop(name, value):
    p = PropertyValue()
    p.Name = name
    p.Value = value
    return p


def describe(e):
    return "%s: %s" % (type(e).__name__, getattr(e, "Message", None) or str(e))


def load(desktop, path):
    url = uno.systemPathToFileUrl(os.path.abspath(path))
    doc = desktop.loadComponentFromURL(
        url,
        "_blank",
        0,
        (
            prop("Hidden", True),
            prop("UpdateDocMode", 0),
            prop("Silent", True),
            # NEVER_EXECUTE (com.sun.star.document.MacroExecMode): a document's own macros never run.
            prop("MacroExecutionMode", 0),
        ),
    )
    if doc is None:
        raise RuntimeError("loadComponentFromURL returned nothing")
    if not doc.supportsService("com.sun.star.text.TextDocument"):
        doc.close(True)
        raise RuntimeError("not loaded as a text document")
    return doc


def all_changes(ctx, doc, command):
    dispatcher = ctx.ServiceManager.createInstanceWithContext("com.sun.star.frame.DispatchHelper", ctx)
    frame = doc.getCurrentController().getFrame()
    dispatcher.executeDispatch(frame, command, "", 0, ())
    return doc.getRedlines().getCount()


def text_boxes(doc):
    """The names of the frames that hold the text of a shape's text box: stetdoc shows a text box as an object, not as
    paragraphs of the body, so these are left out."""
    names = set()
    page = doc.getDrawPage()
    for i in range(page.getCount()):
        shape = page.getByIndex(i)
        try:
            if shape.TextBox and shape.TextBoxContent is not None:
                names.add(shape.TextBoxContent.getName())
        except Exception:
            pass
    return names


def paragraphs(text, out, boxes, seen=None):
    """The paragraphs of a text, a table's cells row by row. A frame anchored to a paragraph (LibreOffice makes one of a
    floating table and of paragraphs with frame properties, which Word keeps in the body before that paragraph) comes
    before the paragraph's text, once."""
    if seen is None:
        seen = set()
    e = text.createEnumeration()
    while e.hasMoreElements():
        el = e.nextElement()
        if el.supportsService("com.sun.star.text.TextTable"):
            for name in el.getCellNames():
                paragraphs(el.getCellByName(name), out, boxes, seen)
        elif el.supportsService("com.sun.star.text.Paragraph"):
            anchored = el.createContentEnumeration("com.sun.star.text.TextContent")
            while anchored.hasMoreElements():
                frame = anchored.nextElement()
                if not frame.supportsService("com.sun.star.text.TextFrame"):
                    continue
                name = frame.getName()
                if name in boxes or name in seen:
                    continue
                seen.add(name)
                paragraphs(frame.getText(), out, boxes, seen)
            out.append(paragraph_text(el))
    return out


# Text portions that stand for no characters of the paragraph's text: a note's anchor (LibreOffice shows its number,
# stetdoc an atom with no characters), a comment's anchor and an object anchored as a character.
SKIPPED = ("Footnote", "Annotation", "AnnotationEnd", "Frame")


def paragraph_text(para):
    parts = []
    e = para.createEnumeration()
    while e.hasMoreElements():
        portion = e.nextElement()
        kind = getattr(portion, "TextPortionType", "Text")
        if kind in SKIPPED:
            continue
        if kind == "ContentControl":
            # The portion of a content control has no characters of its own; its text is the control's.
            parts.append(portion.ContentControl.getString())
            continue
        parts.append(portion.getString())
    return "".join(parts)


def notes(doc):
    out = []
    for coll in (doc.getFootnotes(), doc.getEndnotes()):
        for i in range(coll.getCount()):
            paragraphs(coll.getByIndex(i), out, set())
    return out


def one(ctx, desktop, path, work):
    r = {"path": path, "load": None, "pdf": None, "accept": None, "reject": None, "error": None}
    try:
        doc = load(desktop, path)
    except Exception as e:
        r["load"] = describe(e)
        return r
    try:
        pdf = os.path.join(work, "out.pdf")
        try:
            doc.storeToURL(uno.systemPathToFileUrl(pdf), (prop("FilterName", "writer_pdf_Export"),))
            if not os.path.exists(pdf) or os.path.getsize(pdf) == 0:
                r["pdf"] = "no PDF was written"
        except Exception as e:
            r["pdf"] = describe(e)
        finally:
            if os.path.exists(pdf):
                os.remove(pdf)
        r["left_accept"] = all_changes(ctx, doc, ".uno:AcceptAllTrackedChanges")
        r["accept"] = paragraphs(doc.getText(), [], text_boxes(doc))
        r["notes_accept"] = notes(doc)
    except Exception as e:
        r["error"] = describe(e)
    finally:
        doc.close(True)
    if r["error"] is not None:
        return r
    try:
        doc = load(desktop, path)
        try:
            r["left_reject"] = all_changes(ctx, doc, ".uno:RejectAllTrackedChanges")
            r["reject"] = paragraphs(doc.getText(), [], text_boxes(doc))
            r["notes_reject"] = notes(doc)
        finally:
            doc.close(True)
    except Exception as e:
        r["error"] = describe(e)
    return r


# The run of a note's own number (w:footnoteRef or w:endnoteRef) at the start of a deletion.
NOTE_MARK_IN_DELETION = re.compile(
    r"(<w:del\b[^>]*>)(\s*<w:r\b(?:(?!</w:r>).)*?<w:(?:footnote|endnote)Ref/>\s*</w:r>)", re.DOTALL
)


def ablated(path, variant, work):
    """A copy of the document with one feature taken out, to find out what a mismatch depends on:
    `unique-dates` gives every revision of every part its own date (LibreOffice joins adjacent revisions of one author and
    date into one), `no-comment-ranges` takes out the start and end of every comment's range, and `note-marks-kept` moves
    the run of a note's own number out of a deletion that starts with it."""
    out = os.path.join(work, "ablated.docx")
    counter = [0]

    def date(_m):
        counter[0] += 1
        # Five minutes apart: LibreOffice joins revisions whose dates are a minute or less apart.
        n = counter[0] * 5
        return 'w:date="%04d-%02d-%02dT%02d:%02d:00Z"' % (
            2001 + n // 483840,
            1 + n // 40320 % 12,
            1 + n // 1440 % 28,
            n // 60 % 24,
            n % 60,
        )

    with zipfile.ZipFile(path) as zin, zipfile.ZipFile(out, "w", zipfile.ZIP_DEFLATED) as zout:
        for item in zin.infolist():
            data = zin.read(item.filename)
            if item.filename.startswith("word/") and item.filename.endswith(".xml"):
                text = data.decode("utf-8")
                if variant == "unique-dates":
                    text = re.sub(r'w:date="[^"]*"', date, text)
                elif variant == "no-comment-ranges":
                    text = re.sub(r"<w:commentRange(Start|End)\b[^>]*/>", "", text)
                elif variant == "note-marks-kept":
                    text = NOTE_MARK_IN_DELETION.sub(r"\2\1", text)
                else:
                    raise RuntimeError("unknown variant " + variant)
                data = text.encode("utf-8")
            zout.writestr(item.filename, data)
    return out


def run(*args):
    ctx = XSCRIPTCONTEXT.getComponentContext()  # noqa: F821 (soffice defines it)
    desktop = XSCRIPTCONTEXT.getDesktop()  # noqa: F821
    jobs = os.environ["STETDOC_LO_JOBS"]
    results = os.environ["STETDOC_LO_RESULTS"]
    work = os.environ["STETDOC_LO_WORK"]
    with open(jobs, encoding="utf-8") as f:
        paths = [line.rstrip("\n") for line in f if line.strip()]
    done = 0
    if os.path.exists(results):
        with open(results, encoding="utf-8") as f:
            done = sum(1 for line in f if line.strip())
    with open(results, "a", encoding="utf-8") as out:
        for job in paths[done:]:
            path, _, variant = job.partition("\t")
            try:
                source = ablated(path, variant, work) if variant else path
                r = one(ctx, desktop, source, work)
                r["path"] = path
                r["variant"] = variant or None
            except Exception as e:
                r = {"path": path, "load": None, "pdf": None, "accept": None, "reject": None, "error": describe(e)}
            out.write(json.dumps(r, ensure_ascii=False) + "\n")
            out.flush()
    desktop.terminate()


g_exportedScripts = (run,)
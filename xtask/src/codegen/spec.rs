//! What to generate, apart from the pinned data itself: which schema files to load, which
//! parts get a root, and the few facts the SDK data does not carry.

/// Schema files loaded in full (content models and attributes). Elements the data refers
/// to from other namespaces (DrawingML, VML, math) become opaque: allowed where the
/// schema allows them, their content not validated.
pub const SCHEMA_FILES: &[&str] = &[
    "schemas_openxmlformats_org_wordprocessingml_2006_main",
    "schemas_microsoft_com_office_word_2010_wordml",
    "schemas_microsoft_com_office_word_2012_wordml",
    "schemas_microsoft_com_office_word_2016_wordml_cid",
    "schemas_microsoft_com_office_word_2018_wordml_cex",
    "schemas_microsoft_com_office_word_2018_wordml",
    "schemas_microsoft_com_office_word_2023_wordml_word16du",
    "schemas_microsoft_com_office_word_2015_wordml_symex",
    "schemas_microsoft_com_office_word_2020_wordml_sdtdatahash",
    "schemas_microsoft_com_office_word_2024_wordml_sdtformatlock",
];

pub const XML_NS: &str = "http://www.w3.org/XML/1998/namespace";

/// ISO 29500 Strict URIs and the Transitional namespace each one stands for.
pub const STRICT_ALIASES: &[(&str, &str)] = &[
    (
        "http://purl.oclc.org/ooxml/wordprocessingml/main",
        "http://schemas.openxmlformats.org/wordprocessingml/2006/main",
    ),
    (
        "http://purl.oclc.org/ooxml/officeDocument/relationships",
        "http://schemas.openxmlformats.org/officeDocument/2006/relationships",
    ),
    (
        "http://purl.oclc.org/ooxml/drawingml/main",
        "http://schemas.openxmlformats.org/drawingml/2006/main",
    ),
    (
        "http://purl.oclc.org/ooxml/drawingml/wordprocessingDrawing",
        "http://schemas.openxmlformats.org/drawingml/2006/wordprocessingDrawing",
    ),
    (
        "http://purl.oclc.org/ooxml/drawingml/picture",
        "http://schemas.openxmlformats.org/drawingml/2006/picture",
    ),
    (
        "http://purl.oclc.org/ooxml/officeDocument/math",
        "http://schemas.openxmlformats.org/officeDocument/2006/math",
    ),
];

/// Namespaces that Word reads as another one and that are not Strict: Word 2013's preview wrote `w15`
/// in its 2010/11 namespace (`Comment042` and `Invalid_Word15Comments` of the corpus).
pub const PREVIEW_ALIASES: &[(&str, &str)] = &[(
    "http://schemas.microsoft.com/office/word/2010/11/wordml",
    "http://schemas.microsoft.com/office/word/2012/wordml",
)];

pub struct Part {
    /// Variant of `schema::PartKind`.
    pub variant: &'static str,
    /// `parts/<json>.json` in the pinned input.
    pub json: &'static str,
    /// Content types the SDK data does not list.
    pub extra_content_types: &'static [&'static str],
    pub default_path: &'static str,
}

pub const PARTS: &[Part] = &[
    Part {
        variant: "Document",
        json: "MainDocumentPart",
        extra_content_types: &[
            "application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml",
            "application/vnd.ms-word.document.macroEnabled.main+xml",
            "application/vnd.openxmlformats-officedocument.wordprocessingml.template.main+xml",
            "application/vnd.ms-word.template.macroEnabledTemplate.main+xml",
        ],
        default_path: "word/document.xml",
    },
    Part {
        variant: "Styles",
        json: "StyleDefinitionsPart",
        extra_content_types: &[],
        default_path: "word/styles.xml",
    },
    Part {
        variant: "StylesWithEffects",
        json: "StylesWithEffectsPart",
        extra_content_types: &[],
        default_path: "word/stylesWithEffects.xml",
    },
    Part {
        variant: "Numbering",
        json: "NumberingDefinitionsPart",
        extra_content_types: &[],
        default_path: "word/numbering.xml",
    },
    Part {
        variant: "Comments",
        json: "WordprocessingCommentsPart",
        extra_content_types: &[],
        default_path: "word/comments.xml",
    },
    Part {
        variant: "CommentsExtended",
        json: "WordprocessingCommentsExPart",
        extra_content_types: &[],
        default_path: "word/commentsExtended.xml",
    },
    Part {
        variant: "CommentsIds",
        json: "WordprocessingCommentsIdsPart",
        extra_content_types: &[],
        default_path: "word/commentsIds.xml",
    },
    Part {
        variant: "CommentsExtensible",
        json: "WordCommentsExtensiblePart",
        extra_content_types: &[],
        default_path: "word/commentsExtensible.xml",
    },
    Part {
        variant: "People",
        json: "WordprocessingPeoplePart",
        extra_content_types: &[],
        default_path: "word/people.xml",
    },
    Part {
        variant: "Footnotes",
        json: "FootnotesPart",
        extra_content_types: &[],
        default_path: "word/footnotes.xml",
    },
    Part {
        variant: "Endnotes",
        json: "EndnotesPart",
        extra_content_types: &[],
        default_path: "word/endnotes.xml",
    },
    Part {
        variant: "Header",
        json: "HeaderPart",
        extra_content_types: &[],
        default_path: "word/header1.xml",
    },
    Part {
        variant: "Footer",
        json: "FooterPart",
        extra_content_types: &[],
        default_path: "word/footer1.xml",
    },
    Part {
        variant: "Settings",
        json: "DocumentSettingsPart",
        extra_content_types: &[],
        default_path: "word/settings.xml",
    },
    Part {
        variant: "WebSettings",
        json: "WebSettingsPart",
        extra_content_types: &[],
        default_path: "word/webSettings.xml",
    },
    Part {
        variant: "FontTable",
        json: "FontTablePart",
        extra_content_types: &[],
        default_path: "word/fontTable.xml",
    },
    Part {
        variant: "Glossary",
        json: "GlossaryDocumentPart",
        extra_content_types: &[],
        default_path: "word/glossary/document.xml",
    },
];

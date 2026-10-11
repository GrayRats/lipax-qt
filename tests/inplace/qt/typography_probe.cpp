// Real Qt text layout probe (QTextLayout, QRawFont, QFontDatabase): which font actually draws each run,
// whether glyphs are missing, how kerning/ligatures/combining marks/grapheme clusters/weights/fractional
// sizes behave. Prints one JSON document; the pytest wrapper makes the assertions.
//
//   typography_probe FONT_DIR            - registers every .ttf/.otf of FONT_DIR like the application does
//
// Run it with FONTCONFIG_FILE pointing to an empty configuration for the isolated check (no system fonts),
// and without for the check of what the system adds. Nothing is downloaded or installed.
#include <QGuiApplication>
#include <QDir>
#include <QFont>
#include <QFontDatabase>
#include <QFontMetricsF>
#include <QJsonArray>
#include <QJsonDocument>
#include <QJsonObject>
#include <QRawFont>
#include <QSet>
#include <QTextBoundaryFinder>
#include <QTextLayout>
#include <QTextOption>
#include <algorithm>
#include <iostream>

static QSet<QString> g_bundled;

static QJsonObject shape(const QString &family, const QString &text, int weight = QFont::Normal, double pixelSize = 24.0, bool kerning = true) {
    QFont font(family);
    font.setPixelSize(int(pixelSize));
    font.setWeight(QFont::Weight(weight));
    font.setKerning(kerning);
    QTextLayout layout(text, font);
    layout.beginLayout();
    layout.createLine();
    layout.endLayout();
    QJsonArray runs;
    int missing = 0, glyphs = 0;
    bool bundledOnly = true;
    QString identified;
    for (const QGlyphRun &run : layout.glyphRuns()) {
        const QRawFont raw = run.rawFont();
        int zeros = 0;
        for (quint32 g : run.glyphIndexes()) zeros += (g == 0);
        const QString name = raw.familyName();
        const bool bundled = g_bundled.contains(name);
        bundledOnly = bundledOnly && bundled;
        runs.append(QJsonObject{{"family", name}, {"bundled", bundled}, {"glyphs", run.glyphIndexes().size()}, {"missing", zeros}});
        missing += zeros;
        glyphs += run.glyphIndexes().size();
    }
    return {{"requested", family}, {"text", text}, {"runs", runs}, {"glyphs", glyphs}, {"missing", missing},
            {"bundled_only", bundledOnly}, {"width", layout.maximumWidth()}};
}

static double width(const QString &family, const QString &text, double pixelSize, int weight = QFont::Normal, bool kerning = true) {
    QFont font(family);
    font.setPixelSize(int(pixelSize));
    font.setWeight(QFont::Weight(weight));
    font.setKerning(kerning);
    return QFontMetricsF(font).horizontalAdvance(text);
}

int main(int argc, char **argv) {
    QGuiApplication app(argc, argv);
    if (argc < 2) return 2;
    QJsonObject out;
    out["platform"] = QGuiApplication::platformName();
    out["system_families_before"] = QFontDatabase::families().size();
    QStringList registered;
    for (const QString &name : QDir(argv[1]).entryList({"*.ttf", "*.otf"}, QDir::Files, QDir::Name)) {
        const int id = QFontDatabase::addApplicationFont(QDir(argv[1]).filePath(name));
        if (id < 0) { out["failed_font"] = name; continue; }
        for (const QString &f : QFontDatabase::applicationFontFamilies(id)) { g_bundled.insert(f); if (!registered.contains(f)) registered << f; }
    }
    QJsonArray reg;
    for (const QString &f : registered) reg.append(f);
    out["registered"] = reg;
    out["families_after"] = QFontDatabase::families().size();

    // Which writing systems do the *bundled* families cover (as Qt sees them)?
    QJsonObject coverage;
    const QList<QPair<QString, QFontDatabase::WritingSystem>> systems = {
        {"latin", QFontDatabase::Latin}, {"cyrillic", QFontDatabase::Cyrillic}, {"greek", QFontDatabase::Greek},
        {"japanese", QFontDatabase::Japanese}, {"korean", QFontDatabase::Korean}, {"simplified_chinese", QFontDatabase::SimplifiedChinese},
        {"traditional_chinese", QFontDatabase::TraditionalChinese}, {"arabic", QFontDatabase::Arabic}, {"devanagari", QFontDatabase::Devanagari},
        {"thai", QFontDatabase::Thai}, {"hebrew", QFontDatabase::Hebrew}};
    for (const auto &s : systems) {
        int bundled = 0;
        for (const QString &f : QFontDatabase::families(s.second)) bundled += g_bundled.contains(f);
        coverage[s.first] = bundled;
    }
    out["bundled_families_per_writing_system"] = coverage;

    // Shaping samples. `Noto Sans` for Latin/Cyrillic/Greek, `Noto Sans CJK SC` for CJK.
    QJsonObject samples;
    samples["latin"] = shape("Noto Sans", "Hello, world");
    samples["cyrillic"] = shape("Noto Sans", QString::fromUtf8("Привет, мир"));
    samples["greek"] = shape("Noto Sans", QString::fromUtf8("Ελληνικά"));
    samples["cjk_sc"] = shape("Noto Sans CJK SC", QString::fromUtf8("日本語中文"));
    samples["cjk_serif"] = shape("Noto Serif CJK SC", QString::fromUtf8("日本語中文한글"));
    samples["korean"] = shape("Noto Sans CJK SC", QString::fromUtf8("한글"));
    samples["mixed"] = shape("Noto Sans", QString::fromUtf8("Hello Привет 日本語 한글"));
    samples["mixed_requested_latin_only_font"] = shape("Inter", QString::fromUtf8("Hello 日本語"));
    samples["arabic"] = shape("Noto Sans", QString::fromUtf8("العربية"));
    samples["devanagari"] = shape("Noto Sans", QString::fromUtf8("हिन्दी"));
    samples["thai"] = shape("Noto Sans", QString::fromUtf8("ภาษาไทย"));
    samples["replacement_character"] = shape("Noto Sans", QString::fromUtf8("a�b"));
    samples["private_use"] = shape("Noto Sans", QString::fromUtf8("ab"));
    out["samples"] = samples;

    // Combining marks and grapheme clusters: one user-perceived character, however many code points.
    QJsonObject graphemes;
    for (const QString &text : {QString::fromUtf8("é"), QString::fromUtf8("й"), QString::fromUtf8("й"), QString::fromUtf8("\U0001F1F7\U0001F1FA"), QString::fromUtf8("\U0001F468‍\U0001F469‍\U0001F467")}) {
        QTextBoundaryFinder finder(QTextBoundaryFinder::Grapheme, text);
        int clusters = 0;
        while (finder.toNextBoundary() != -1) ++clusters;
        graphemes[text.toUtf8().toHex()] = QJsonObject{{"code_units", text.size()}, {"clusters", clusters},
            {"width_24px", width("Noto Sans", text, 24)}, {"precomposed_width_24px", width("Noto Sans", QString::fromUtf8("é"), 24)}};
    }
    out["graphemes"] = graphemes;

    // Kerning: widths with and without it; ligatures: glyph counts with the font's own substitution.
    QJsonObject kern;
    kern["AVAVAV_on"] = width("Inter", "AVAVAV", 48, QFont::Normal, true);
    kern["AVAVAV_off"] = width("Inter", "AVAVAV", 48, QFont::Normal, false);
    kern["ffi_glyphs_inter"] = shape("Inter", "office")["glyphs"];
    kern["fi_glyphs_noto_serif"] = shape("Noto Serif", "fi")["glyphs"];
    kern["fi_glyphs_fira_code"] = shape("Fira Code", "->=>")["glyphs"];
    out["kerning_and_ligatures"] = kern;

    // Fractional scaling: 13.5 pt against 27 pt (and exact doubling of widths within hinting tolerance).
    QJsonObject fractional;
    for (double pt : {9.0, 13.5, 27.0}) {
        QFont f("Noto Sans"); f.setPointSizeF(pt);
        QFontMetricsF m(f);
        fractional[QString::number(pt)] = QJsonObject{{"width", m.horizontalAdvance("Hello Привет")}, {"ascent", m.ascent()}, {"height", m.height()}};
    }
    out["fractional_point_sizes"] = fractional;

    // Wrapping and line height with mixed weights on one paragraph.
    {
        QTextLayout layout(QString::fromUtf8("Normal text and bold words and Привет мир that need to wrap over several lines."), QFont("Noto Sans", 12));
        QFont bold("Noto Sans", 12); bold.setWeight(QFont::Bold);
        QTextLayout::FormatRange range; range.start = 16; range.length = 14; range.format.setFont(bold);
        layout.setFormats({range});
        QTextOption option; option.setWrapMode(QTextOption::WordWrap); layout.setTextOption(option);
        layout.beginLayout();
        QJsonArray lines; double y = 0;
        for (;;) {
            QTextLine line = layout.createLine();
            if (!line.isValid()) break;
            line.setLineWidth(180); line.setPosition(QPointF(0, y));
            lines.append(QJsonObject{{"width", line.naturalTextWidth()}, {"height", line.height()}, {"ascent", line.ascent()}, {"y", y}});
            y += line.height();
        }
        layout.endLayout();
        out["wrapped_mixed_weight"] = QJsonObject{{"lines", lines}, {"max_width", 180}};
    }
    std::cout << QJsonDocument(out).toJson(QJsonDocument::Indented).toStdString() << std::endl;
}

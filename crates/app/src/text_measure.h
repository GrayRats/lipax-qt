#pragma once
#include <QFontMetricsF>
#include <QTextLayout>
#include <QTextOption>
#include <QString>
#include <algorithm>
#include <cmath>

namespace lipa {
struct TextExtent {
    double width = 0;
    double height = 0;
    double spacing = 0;
    int lines = 0;
};
// The application bridge and the offline integration harness use identical fonts.
inline QFont textFont(const QString &family, int weight, bool italic, double px, double letterSpacing) {
    QFont font(family);
    font.setPixelSize(std::max(1, int(std::floor(px))));
    font.setWeight(QFont::Weight(weight));
    font.setItalic(italic);
    font.setStyleStrategy(QFont::ContextFontMerging);
    font.setKerning(true);
    font.setLetterSpacing(QFont::AbsoluteSpacing, letterSpacing);
    return font;
}
// Count actual shaped lines; boundingRect height / spacing is not a line count.
// Match QML Text.Wrap (word boundary with anywhere fallback), including hard breaks.
inline TextExtent measureText(const QString &text, const QFont &font, double width, int wrap) {
    const QFontMetricsF metrics(font);
    TextExtent result;
    result.spacing = metrics.lineSpacing();
    QTextOption option;
    option.setWrapMode(wrap == 2 ? QTextOption::NoWrap : wrap == 1 ? QTextOption::WrapAnywhere
                                                          : QTextOption::WrapAtWordBoundaryOrAnywhere);
    QString normalized = text;
    normalized.replace(QStringLiteral("\r\n"), QStringLiteral("\n"));
    normalized.replace(QChar('\r'), QChar('\n'));
    normalized.replace(QChar(0x2029), QChar('\n'));
    const auto paragraphs = normalized.split(QChar('\n'), Qt::KeepEmptyParts);
    for (const auto &paragraph : paragraphs) {
        QTextLayout layout(paragraph, font);
        layout.setTextOption(option);
        layout.beginLayout();
        int count = 0;
        while (true) {
            auto line = layout.createLine();
            if (!line.isValid()) break;
            line.setLineWidth(wrap == 2 ? 1e7 : std::max(1.0, width));
            result.width = std::max(result.width, line.naturalTextWidth());
            result.height += line.height();
            result.spacing = std::max(result.spacing, line.height() + line.leading());
            ++count;
        }
        layout.endLayout();
        if (!count) result.height += metrics.height();
        result.lines += std::max(1, count);
    }
    return result;
}
}

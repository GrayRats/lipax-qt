#pragma once
#include <QGuiApplication>
#include <QIcon>
#include <QFontDatabase>
#include <QDir>
#include <QDebug>
inline void configureLipaApplication() {
    QGuiApplication::setDesktopFileName(QStringLiteral("io.lipa.Translator"));
    QGuiApplication::setWindowIcon(QIcon(QStringLiteral(":/lipa/icon.svg")));
}

inline QString loadLipaFonts(const QString &directory) {
    const char *names[] = {
        "inter-Inter[opsz,wght].ttf", "inter-Inter-Italic[opsz,wght].ttf",
        "notosans-NotoSans[wdth,wght].ttf", "notosans-NotoSans-Italic[wdth,wght].ttf",
        "notoserif-NotoSerif[wdth,wght].ttf", "notoserif-NotoSerif-Italic[wdth,wght].ttf",
        "jetbrainsmono-JetBrainsMono[wght].ttf", "jetbrainsmono-JetBrainsMono-Italic[wght].ttf",
        "NotoSansCJK-Regular.ttc", "NotoSansCJK-Bold.ttc",
        "NotoSerifCJK-Regular.ttc", "NotoSerifCJK-Bold.ttc",
    };
    QStringList families;
    for (const char *name : names) {
        const QString path = QDir(directory).filePath(QString::fromUtf8(name));
        const int id = QFontDatabase::addApplicationFont(path);
        if (id < 0) {
            qCritical().noquote() << "inplace.font: failed to load bundled font path=" << path;
            continue;
        }
        for (const QString &family : QFontDatabase::applicationFontFamilies(id)) {
            if (!families.contains(family)) families.append(family);
        }
    }
    qInfo().noquote() << "inplace.font: bundled families=" << families.join(", ");
    return families.join("\n");
}

#include <QClipboard>
#include <QRegion>
#include <QWindow>
#include <QString>
#include <QPainterPath>
#include <KWindowEffects>
#include "rust/cxx.h"

// Blur behind the translation overlay, done by KWin (org_kde_kwin_blur). The strength is
// KWin's global setting; elsewhere this is a no-op and only the tint is drawn.
inline void configureOverlayBlur(bool enable, int radius) {
    for (QWindow *window : QGuiApplication::allWindows()) {
        if (window->objectName() != QStringLiteral("translationOverlay")) continue;
        QRegion region;
        if (enable && radius > 0) {
            QPainterPath path;
            path.addRoundedRect(QRectF(0, 0, window->width(), window->height()), radius, radius);
            region = QRegion(path.toFillPolygon().toPolygon());
        }
        // An empty region with `enable` blurs the whole window.
        KWindowEffects::enableBlurBehind(window, enable, region);
    }
}
// `rects` is a flat list of x, y, w, h computed by the overlay QML: the only parts that keep
// receiving input while clicks pass through to the game.
inline void configureOverlayInput(bool passthrough, rust::Slice<const int32_t> rects) {
    for (QWindow *window : QGuiApplication::allWindows()) {
        if (window->objectName() != QStringLiteral("translationOverlay")) continue;
        // On Wayland QWindow::mask defines the input region. Some X11 backends
        // also clip rendering, so preserve the complete window on that fallback.
        if (passthrough && rects.size() >= 4 && QGuiApplication::platformName().startsWith(QStringLiteral("wayland"))) {
            QRegion region;
            for (size_t i = 0; i + 3 < rects.size(); i += 4)
                region += QRect(rects[i], rects[i + 1], rects[i + 2], rects[i + 3]);
            window->setMask(region);
        } else { window->setMask(QRegion()); }
    }
}
inline void copyLipaText(const QString &text) { QGuiApplication::clipboard()->setText(text); }

// ── Font metrics for fitting the in-place translation (layout::fit::TextMeasure) ──
#include <QFont>
#include <QFontMetricsF>
#include <cmath>
inline QFont lipaFont(rust::Str family, int weight, bool italic, double px, double letterSpacing) {
    QFont f(QString::fromUtf8(family.data(), int(family.size())));
    f.setStyleStrategy(QFont::NoFontMerging);
    f.setPixelSize(qMax(1, int(std::floor(px))));
    f.setWeight(QFont::Weight(weight));
    f.setItalic(italic);
    f.setLetterSpacing(QFont::AbsoluteSpacing, letterSpacing);
    return f;
}
// wrap: 0 word, 1 anywhere, 2 none. out: width, height, line spacing, line count.
inline void measureLipaText(rust::Str family, int weight, bool italic, double px, double letterSpacing,
                            double width, int wrap, rust::Str text, rust::Slice<double> out) {
    const QFontMetricsF fm(lipaFont(family, weight, italic, px, letterSpacing));
    const int flags = wrap == 0 ? Qt::TextWordWrap : wrap == 1 ? Qt::TextWrapAnywhere : 0;
    const QRectF r = fm.boundingRect(QRectF(0, 0, wrap == 2 ? 1e7 : width, 1e7), flags,
                                     QString::fromUtf8(text.data(), int(text.size())));
    const double spacing = fm.lineSpacing();
    out[0] = r.width();
    out[1] = r.height();
    out[2] = spacing;
    out[3] = spacing > 0 ? qMax(1.0, std::round(r.height() / spacing)) : 1.0;
}
// Cap height relative to the pixel size; 0.7 if the font does not report it.
inline double lipaCapRatio(rust::Str family, int weight, bool italic) {
    const QFontMetricsF fm(lipaFont(family, weight, italic, 100.0, 0.0));
    const double cap = fm.capHeight();
    return cap > 0 ? cap / 100.0 : 0.7;
}

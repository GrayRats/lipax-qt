#pragma once
#include <QGuiApplication>
#include <QIcon>
inline void configureLipaApplication() {
    QGuiApplication::setDesktopFileName(QStringLiteral("io.lipa.Translator"));
    QGuiApplication::setWindowIcon(QIcon(QStringLiteral(":/lipa/icon.svg")));
}

#include <QClipboard>
#include <QRegion>
#include <QWindow>
#include <QString>
#include "rust/cxx.h"
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

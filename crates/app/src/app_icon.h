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
inline void configureOverlayInput(bool pinned, bool passthrough, int edge) {
    for (QWindow *window : QGuiApplication::allWindows()) {
        if (window->objectName() != QStringLiteral("translationOverlay")) continue;
        // On Wayland QWindow::mask defines the input region. Some X11 backends
        // also clip rendering, so preserve the complete window on that fallback.
        if (pinned && passthrough && QGuiApplication::platformName().startsWith(QStringLiteral("wayland"))) {
            const QRect full(0, 0, window->width(), window->height());
            const int grip = qMax(edge, 10);
            QRegion region(full);
            region -= full.adjusted(grip, grip, -grip, -grip);
            region += QRect(qMax(0, window->width()-42), 0, 42, 30);
            window->setMask(region);
        } else { window->setMask(QRegion()); }
    }
}
inline void copyLipaText(const QString &text) { QGuiApplication::clipboard()->setText(text); }

#pragma once
#include <QGuiApplication>
#include <QIcon>
inline void configureLipaApplication() {
    QGuiApplication::setDesktopFileName(QStringLiteral("io.lipa.Translator"));
    QGuiApplication::setWindowIcon(QIcon(QStringLiteral(":/lipa/icon.svg")));
}

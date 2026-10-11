// Test-only IPC adapter for the production Qt text helper. No application dependency.
#include "text_measure.h"
#include <QGuiApplication>
#include <QFontDatabase>
#include <QDir>
#include <QJsonDocument>
#include <QJsonObject>
#include <iostream>
#include <string>

int main(int argc, char **argv) {
    QGuiApplication app(argc, argv);
    if (argc != 2) return 2;
    const QDir fonts(QString::fromLocal8Bit(argv[1]));
    int loaded = 0;
    for (const auto &name : fonts.entryList({"*.ttf", "*.otf", "*.ttc"}, QDir::Files)) {
        if (QFontDatabase::addApplicationFont(fonts.filePath(name)) >= 0) ++loaded;
    }
    if (!loaded) return 3;
    std::string input;
    while (std::getline(std::cin, input)) {
        QJsonParseError error;
        const auto doc = QJsonDocument::fromJson(QByteArray::fromStdString(input), &error);
        if (error.error != QJsonParseError::NoError || !doc.isObject()) return 4;
        const auto request = doc.object();
        const auto font = lipa::textFont(request["family"].toString(), request["weight"].toInt(),
            request["italic"].toBool(), request["px"].toDouble(), request["letter_spacing"].toDouble());
        QJsonObject result;
        if (request["operation"].toString() == "cap") {
            const double cap = QFontMetricsF(font).capHeight();
            result["cap_ratio"] = cap > 0 ? cap / 100.0 : 0.7;
        } else {
            const auto extent = lipa::measureText(request["text"].toString(), font,
                request["width"].toDouble(), request["wrap"].toInt());
            result["width"] = extent.width;
            result["height"] = extent.height;
            result["line_spacing"] = extent.spacing;
            result["lines"] = extent.lines;
        }
        std::cout << QJsonDocument(result).toJson(QJsonDocument::Compact).constData() << std::endl;
    }
}

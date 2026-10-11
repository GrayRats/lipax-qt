#include "text_measure.h"
#include <QGuiApplication>
#include <QFont>
#include <QElapsedTimer>
#include <iostream>
#include <cassert>
int main(int argc, char **argv) {
    QGuiApplication app(argc, argv);
    QFont font("Noto Sans"); font.setPixelSize(20); font.setStyleStrategy(QFont::ContextFontMerging);
    assert(lipa::measureText("one\ntwo\n", font, 400, 2).lines == 3);
    assert(lipa::measureText("one\r\ntwo", font, 400, 2).lines == 2);
    assert(lipa::measureText("", font, 400, 2).lines == 1);
    auto longWord = lipa::measureText("supercalifragilisticexpialidocious", font, 60, 0);
    assert(longWord.lines > 1 && longWord.width <= 60.01);
    for (const auto &text : {QString::fromUtf8("Hello Привет 日本語"), QString::fromUtf8("á é office العربية")}) {
        auto extent = lipa::measureText(text, font, 400, 0);
        assert(extent.lines >= 1 && extent.width > 0 && extent.height > 0);
    }
    QElapsedTimer timer;
    const QString text = QString::fromUtf8("Hello Привет 日本語 office supercalifragilisticexpialidocious\nSecond line");
    timer.start();
    for (int i=0; i<10000; ++i) {
        const QFontMetricsF fm(font);
        auto rect = fm.boundingRect(QRectF(0,0,150,1e7), Qt::TextWordWrap, text);
        if (rect.width() < 0) return 1;
    }
    const double baseline = timer.nsecsElapsed()/1e6/10000;
    timer.restart();
    for (int i=0; i<10000; ++i) {
        auto extent = lipa::measureText(text, font, 150, 0);
        if (extent.width < 0) return 1;
    }
    std::cout << "{\"baseline_ms\":" << baseline << ",\"final_ms\":" << timer.nsecsElapsed()/1e6/10000 << ",\"iterations\":10000}" << std::endl;
}

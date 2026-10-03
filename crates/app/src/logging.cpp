#include "lipa/src/logging.cxx.h"
#include <QLoggingCategory>
#include <QMessageLogger>
#include <QQmlComponent>
#include <QQmlEngine>
#include <memory>

namespace lipax {
void installQtLogHandler() {
    qInstallMessageHandler([](QtMsgType type, const QMessageLogContext &context, const QString &message) {
        const auto utf8 = message.toUtf8();
        logQtMessage(static_cast<int>(type), context.category ? context.category : "qt",
                     rust::Str(utf8.constData(), utf8.size()), context.file ? context.file : "",
                     context.line);
    });
}

// Called by subprocess tests; exercises Qt's real handler, including qFatal/abort.
void emitQtLogProbe(bool fatal) {
    const QMessageLogger logger(__FILE__, __LINE__, "emitQtLogProbe", "lipax.test");
    logger.debug("probe-qt-debug");
    logger.info("probe-qt-info");
    logger.warning("probe-qt-warning");
    logger.critical("probe-qt-critical");
    if (fatal) logger.fatal("probe-qt-fatal");
}

void emitQmlLogProbe() {
    QQmlEngine engine;
    QQmlComponent component(&engine);
    component.setData(R"(
        import QtQml
        QtObject {
            property int invalid: missingProbeValue
            Component.onCompleted: {
                console.debug("probe-qml-debug")
                console.warn("probe-qml-warning")
                console.error("probe-qml-error")
            }
        }
    )", QUrl("file:///lipax-logging-probe.qml"));
    const std::unique_ptr<QObject> object(component.create());
}
}

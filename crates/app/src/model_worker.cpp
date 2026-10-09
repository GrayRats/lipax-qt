#include "model_worker.h"
#include "lipa/src/model_worker.cxx.h"
#include <QThread>
#include <QPointer>
#include <vector>
#include <algorithm>

// Accessed only on the GUI thread. QPointer tolerates finished/deleteLater workers.
static std::vector<QPointer<QThread>> threads;
void startModelThread(rust::Box<ModelJob> job) {
    threads.erase(std::remove_if(threads.begin(), threads.end(),
        [](const auto &thread) { return thread.isNull(); }), threads.end());
    auto *thread = QThread::create([job = std::move(job)]() mutable {
        run_model_job(std::move(job));
    });
    threads.emplace_back(thread);
    QObject::connect(thread, &QThread::finished, thread, &QObject::deleteLater);
    thread->start();
}
void joinModelThreads() {
    for (auto &thread : threads) {
        if (thread) { thread->wait(); delete thread.data(); }
    }
    threads.clear();
}

// PROTOTYPE, throwaway. Cold-start spike for "Cold-start spike for the top stacks" (acastaner/zimged#9).
// Qt 6 Widgets: decode starts on a worker thread before QApplication exists, at display size
// (libjpeg DCT scaling through QImageReader::setScaledSize), and is painted on the CPU raster backing store.
//
// Env:
//   SPIKE_T0_NS     wall-clock ns at launch (set by the bench driver); otherwise t0 = main() entry
//   SPIKE_EXIT=1    quit right after the first image is painted
//   SPIKE_NAV=ms,n  after the first image, press Right every <ms> ms, n times, then quit (0 = as soon as painted)
#include <QApplication>
#include <QCollator>
#include <QDir>
#include <QFileInfo>
#include <QImageReader>
#include <QKeyEvent>
#include <QPainter>
#include <QTimer>
#include <QWidget>
#include <chrono>
#include <cstdio>
#include <future>
#include <map>
#include <mutex>
#include <thread>

static long long nowNs() {
    return std::chrono::duration_cast<std::chrono::nanoseconds>(std::chrono::system_clock::now().time_since_epoch()).count();
}
static long long T0;
static void mark(const char *what, double extra = -1) {
    double ms = (nowNs() - T0) / 1e6;
    if (extra >= 0) std::printf("%s %.2f %.2f\n", what, ms, extra);
    else std::printf("%s %.2f\n", what, ms);
    std::fflush(stdout);
}

static const QSize kBox(1600, 1000);  // decode target = the window size (fit-to-window); full resolution can follow in the background

static QImage decodeForDisplay(const QString &path) {
    QImageReader r(path);
    r.setAutoTransform(true);
    QSize s = r.size();
    if (s.isValid() && (s.width() > kBox.width() || s.height() > kBox.height()))
        r.setScaledSize(s.scaled(kBox, Qt::KeepAspectRatio));
    QImage img = r.read();
    return img.convertToFormat(QImage::Format_RGB32);
}

class Viewer : public QWidget {
public:
    QStringList siblings;
    int idx = -1;
    QImage shown;
    std::map<int, std::shared_future<QImage>> cache;
    long long navStart = 0;
    bool firstImagePainted = false;
    bool exitAfterFirst = false;
    int navInterval = -1, navLeft = 0;

    void showIndex(int i) {
        if (i < 0 || i >= siblings.size()) return;
        idx = i;
        auto it = cache.find(i);
        if (it == cache.end()) it = cache.emplace(i, prefetch(i)).first;
        auto fut = it->second;
        // poll until decoded; keeps the UI thread free
        auto poll = std::make_shared<std::function<void()>>();
        *poll = [this, fut, i, poll]() {
            if (idx != i) return;
            if (fut.wait_for(std::chrono::seconds(0)) == std::future_status::ready) {
                shown = fut.get();
                update();
                prefetchAround(i);
            } else QTimer::singleShot(1, this, *poll);
        };
        (*poll)();
    }
    std::shared_future<QImage> prefetch(int i) {
        QString p = siblings[i];
        return std::async(std::launch::async, [p] { return decodeForDisplay(p); }).share();
    }
    void prefetchAround(int i) {
        for (int j : {i + 1, i - 1, i + 2})
            if (j >= 0 && j < siblings.size() && !cache.count(j)) cache.emplace(j, prefetch(j));
        for (auto it = cache.begin(); it != cache.end();)  // keep a small window
            it = (std::abs(it->first - i) > 2) ? cache.erase(it) : std::next(it);
    }
    void scanSiblings(const QString &path) {
        QFileInfo fi(path);
        QDir d = fi.absoluteDir();
        QStringList names = d.entryList({"*.jpg", "*.jpeg", "*.png", "*.webp", "*.gif", "*.bmp", "*.tif", "*.tiff"}, QDir::Files);
        QCollator col; col.setNumericMode(true); col.setCaseSensitivity(Qt::CaseInsensitive);
        std::sort(names.begin(), names.end(), [&](const QString &a, const QString &b) { return col.compare(a, b) < 0; });
        for (auto &n : names) siblings << d.absoluteFilePath(n);
        idx = siblings.indexOf(fi.absoluteFilePath());
        mark("siblings_scanned", siblings.size());
    }

protected:
    void paintEvent(QPaintEvent *) override {
        QPainter p(this);
        p.fillRect(rect(), QColor(17, 18, 20));
        if (shown.isNull()) return;
        p.setRenderHint(QPainter::SmoothPixmapTransform);
        QSize s = shown.size().scaled(size(), Qt::KeepAspectRatio);
        p.drawImage(QRect(QPoint((width() - s.width()) / 2, (height() - s.height()) / 2), s), shown);
        p.end();
        if (!firstImagePainted) {
            firstImagePainted = true;
            mark("first_image_painted");
            if (exitAfterFirst) { QTimer::singleShot(0, qApp, &QApplication::quit); return; }
            QTimer::singleShot(0, this, [this] { afterFirst(); });
        } else if (navStart) {
            std::printf("switch_ms %.2f %d\n", (nowNs() - navStart) / 1e6, idx);
            std::fflush(stdout);
            navStart = 0;
            if (navInterval >= 0) QTimer::singleShot(navInterval, this, [this] { autoNav(); });
        }
    }
    void keyPressEvent(QKeyEvent *e) override {
        if (e->key() == Qt::Key_Right || e->key() == Qt::Key_Left) {
            navStart = nowNs();
            showIndex(idx + (e->key() == Qt::Key_Right ? 1 : -1));
        }
    }
    void wheelEvent(QWheelEvent *e) override {
        navStart = nowNs();
        showIndex(idx + (e->angleDelta().y() < 0 ? 1 : -1));
    }

public:
    QString firstPath;
    void afterFirst() {
        // deferred work: everything that is not needed for the first pixels
        scanSiblings(firstPath);
        cache.emplace(idx, std::async(std::launch::deferred, [img = shown] { return img; }).share());
        prefetchAround(idx);
        if (navInterval >= 0) QTimer::singleShot(std::max(navInterval, 300), this, [this] { autoNav(); });
    }
    void autoNav() {
        if (navLeft-- <= 0 || idx + 1 >= siblings.size()) { qApp->quit(); return; }
        QKeyEvent ev(QEvent::KeyPress, Qt::Key_Right, Qt::NoModifier);
        QApplication::sendEvent(this, &ev);
    }
};

int main(int argc, char **argv) {
    const char *t0 = std::getenv("SPIKE_T0_NS");
    T0 = t0 ? std::atoll(t0) : nowNs();
    mark("main");
    if (argc < 2) { std::fprintf(stderr, "usage: zimged-spike-qt <image>\n"); return 2; }
    QString path = QString::fromLocal8Bit(argv[1]);
    // start decoding before anything else: it runs in parallel with platform/window setup
    std::shared_future<QImage> first = std::async(std::launch::async, [path] {
        QImage i = decodeForDisplay(path);
        mark("decoded");
        return i;
    }).share();

    QApplication app(argc, argv);
    mark("qapplication");
    Viewer w;
    w.firstPath = path;
    w.exitAfterFirst = std::getenv("SPIKE_EXIT") != nullptr;
    if (const char *nav = std::getenv("SPIKE_NAV")) std::sscanf(nav, "%d,%d", &w.navInterval, &w.navLeft);
    w.setWindowTitle(QFileInfo(path).fileName() + " - zimged spike (Qt)");
    w.resize(1600, 1000);
    w.show();
    mark("window_shown");
    QTimer poll;
    QObject::connect(&poll, &QTimer::timeout, [&] {
        if (first.wait_for(std::chrono::seconds(0)) == std::future_status::ready) {
            poll.stop();
            w.shown = first.get();
            w.update();
        }
    });
    poll.start(1);
    return app.exec();
}

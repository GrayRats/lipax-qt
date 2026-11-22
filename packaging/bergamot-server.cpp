// LipaX: a long-lived front end over the bergamot-translator library.
//
// The stock `bergamot` CLI reads stdin until EOF, translates once and exits, so every translation pays for the
// process start and the model load (about 0.17 s measured). This program loads the model once and then answers
// requests until its stdin is closed.
//
// Protocol (binary, over stdin/stdout; stderr is free for diagnostics):
//   server -> client, once the model is loaded:  "LIPAX-BERGAMOT-SERVER 1\n"
//   client -> server, per request:               4-byte big-endian length N, then N bytes of UTF-8 text
//   server -> client, per request:               4-byte big-endian length M, then M bytes of UTF-8 translation
// The text is what the CLI would read from stdin; the translation is what it would print. The server exits 0 when
// stdin reaches EOF at a frame boundary, and non-zero (message on stderr) on any other failure.
//
// This file is copied into the bergamot-translator checkout and built next to the CLI by
// packaging/build-bergamot.sh; it is not part of the Rust workspace.
#include <cstdint>
#include <cstdio>
#include <future>
#include <iostream>
#include <string>

#include "translator/parser.h"
#include "translator/response.h"
#include "translator/response_options.h"
#include "translator/service.h"

namespace {

constexpr uint32_t kMaxFrame = 16u * 1024u * 1024u;

// Returns false on a clean EOF before the first byte, throws on a truncated frame.
bool readExact(std::FILE *in, void *buffer, size_t size, bool eofIsClean) {
  size_t done = std::fread(buffer, 1, size, in);
  if (done == size) return true;
  if (done == 0 && eofIsClean && std::feof(in)) return false;
  throw std::runtime_error("stdin closed in the middle of a request");
}

void writeAll(std::FILE *out, const void *buffer, size_t size) {
  if (std::fwrite(buffer, 1, size, out) != size) throw std::runtime_error("cannot write to stdout");
}

}  // namespace

int main(int argc, char *argv[]) {
  using namespace marian::bergamot;
  try {
    ConfigParser<AsyncService> configParser("LipaX Bergamot server", /*multiOpMode=*/false);
    configParser.parseArgs(argc, argv);
    auto &config = configParser.getConfig();

    AsyncService service(config.serviceConfig);
    auto options = parseOptionsFromFilePath(config.modelConfigPaths.front());
    std::shared_ptr<TranslationModel> model = service.createCompatibleModel(options);
    ResponseOptions responseOptions;

    // The first translation of a fresh model spends ~0.2 s on lazy initialization; do it before announcing readiness.
    {
      std::promise<Response> warmup;
      std::future<Response> done = warmup.get_future();
      service.translate(model, std::string("Hello."), [&warmup](Response &&response) { warmup.set_value(std::move(response)); },
                        responseOptions);
      done.get();
    }

    static const char kReady[] = "LIPAX-BERGAMOT-SERVER 1\n";
    writeAll(stdout, kReady, sizeof(kReady) - 1);
    std::fflush(stdout);

    for (;;) {
      unsigned char header[4];
      if (!readExact(stdin, header, sizeof(header), /*eofIsClean=*/true)) return 0;
      uint32_t length = (uint32_t(header[0]) << 24) | (uint32_t(header[1]) << 16) | (uint32_t(header[2]) << 8) | header[3];
      if (length > kMaxFrame) throw std::runtime_error("request is too large");
      std::string input(length, '\0');
      if (length > 0) readExact(stdin, input.data(), length, /*eofIsClean=*/false);

      std::promise<Response> promise;
      std::future<Response> future = promise.get_future();
      service.translate(model, std::move(input), [&promise](Response &&response) { promise.set_value(std::move(response)); },
                        responseOptions);
      Response response = future.get();

      const std::string &text = response.target.text;
      if (text.size() > kMaxFrame) throw std::runtime_error("response is too large");
      uint32_t size = static_cast<uint32_t>(text.size());
      unsigned char reply[4] = {static_cast<unsigned char>(size >> 24), static_cast<unsigned char>(size >> 16),
                                static_cast<unsigned char>(size >> 8), static_cast<unsigned char>(size)};
      writeAll(stdout, reply, sizeof(reply));
      writeAll(stdout, text.data(), text.size());
      std::fflush(stdout);
    }
  } catch (const std::exception &error) {
    std::fprintf(stderr, "bergamot-server: %s\n", error.what());
    return 1;
  }
}

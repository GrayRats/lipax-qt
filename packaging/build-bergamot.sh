#!/usr/bin/env bash
# Pinned Mozilla engine; keep source/build caches outside the source archive.
# prepare performs network access. build only compiles the prepared checkout.
set -euo pipefail
[[ $# == 2 ]] || { echo "Usage: $0 prepare|build target-directory" >&2; exit 2; }
lipa_engine_target=$(realpath -m -- "$2")
lipa_engine_source="$lipa_engine_target/bergamot-source"
lipa_engine_build="$lipa_engine_target/bergamot-build"
lipa_engine_commit=5ae1b1ebb3fa9a3eabed8a64ca6798154bd486eb
lipa_script_dir=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)
case $1 in
    prepare)
        if [[ ! -d $lipa_engine_source/.git ]]; then
            git clone --filter=blob:none https://github.com/mozilla/bergamot-translator.git "$lipa_engine_source"
        fi
        if ! git -C "$lipa_engine_source" cat-file -e "$lipa_engine_commit^{commit}"; then
            git -C "$lipa_engine_source" fetch origin "$lipa_engine_commit"
        fi
        git -C "$lipa_engine_source" checkout --detach "$lipa_engine_commit"
        git -C "$lipa_engine_source" submodule update --init --depth 1 3rd_party/marian-dev 3rd_party/ssplit-cpp
        # No training corpora, CUDA libraries, Python bindings or regression-test downloads.
        git -C "$lipa_engine_source/3rd_party/marian-dev" submodule update --init --recursive --depth 1 \
            src/3rd_party/intgemm src/3rd_party/sentencepiece src/3rd_party/onnxjs
        # Explicit standard headers / constructor syntax required by GCC 16.
        for lipa_header in broadcast_utils.h shape_utils.h; do
            lipa_header_path="$lipa_engine_source/3rd_party/marian-dev/src/3rd_party/onnxjs/src/wasm-ops/utils/$lipa_header"
            if ! grep -q '^#include <stddef.h>' "$lipa_header_path"; then
                sed -i '1i#include <stddef.h>' "$lipa_header_path"
            fi
        done
        sed -i 's/registry_t<Mutex>(/registry_t(/g' "$lipa_engine_source/3rd_party/marian-dev/src/3rd_party/spdlog/details/registry.h"
        # Submodules are fetched explicitly in prepare, never implicitly during compilation.
        sed -i '/^execute_process(COMMAND git submodule update --init --recursive --no-fetch/,/WORKING_DIRECTORY .*})/d' "$lipa_engine_source/3rd_party/marian-dev/CMakeLists.txt"
        # Keep upstream warnings visible, but do not promote new compiler warnings to errors.
        sed -i 's/-Wall; -Werror; -Wextra;/-Wall; -Wextra;/' "$lipa_engine_source/3rd_party/marian-dev/CMakeLists.txt"
        # LipaX's long-lived front end (model loaded once) is built next to the stock CLI.
        install -m644 "$lipa_script_dir/bergamot-server.cpp" "$lipa_engine_source/app/bergamot-server.cpp"
        if ! grep -q bergamot-server "$lipa_engine_source/app/CMakeLists.txt"; then
            printf '%s\n' 'add_executable(bergamot-server bergamot-server.cpp)' \
                'target_link_libraries(bergamot-server PRIVATE bergamot-translator)' >> "$lipa_engine_source/app/CMakeLists.txt"
        fi
        ;;
    build)
        [[ $(git -C "$lipa_engine_source" rev-parse HEAD) == "$lipa_engine_commit" ]]
        cmake -S "$lipa_engine_source" -B "$lipa_engine_build" -G Ninja \
            -DCMAKE_BUILD_TYPE=Release -DCMAKE_POLICY_VERSION_MINIMUM=3.5 \
            -DUSE_WASM_COMPATIBLE_SOURCE=ON -DGIT_SUBMODULE=OFF \
            -DCOMPILE_TESTS=OFF -DCOMPILE_UNIT_TESTS=OFF -DBUILD_ARCH=x86-64 \
            -DCMAKE_CXX_FLAGS='-include cstdint -include stddef.h' \
            -DSSPLIT_USE_INTERNAL_PCRE2=OFF
        # Pick up an edited bergamot-server.cpp even when prepare is not re-run.
        install -m644 "$lipa_script_dir/bergamot-server.cpp" "$lipa_engine_source/app/bergamot-server.cpp"
        cmake --build "$lipa_engine_build" --target bergamot bergamot-server --parallel "${LIPAX_BUILD_JOBS:-6}"
        "$lipa_engine_build/app/bergamot" --help >/dev/null
        "$lipa_engine_build/app/bergamot-server" --help >/dev/null
        ;;
    *) echo "Unknown action: $1" >&2; exit 2 ;;
esac

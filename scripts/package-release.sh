#!/usr/bin/env bash
# Package prebuilt Linux binaries with documentation and dependency licenses.
set -euo pipefail

fail() { printf '%s\n' "$*" >&2; exit 1; }
for pomni_command in cargo rustc git jq objdump tar sha256sum find install mktemp sort uname realpath; do
    command -v "$pomni_command" >/dev/null 2>&1 || fail "Comando necessário não encontrado: $pomni_command"
done
pomni_root=$(dirname -- "$(dirname -- "$(realpath -- "${BASH_SOURCE[0]}")")")
cd -- "$pomni_root"
pomni_host=$(rustc -vV | sed -n 's/^host: //p')
case "$pomni_host" in *linux-gnu*) ;; *) fail 'Este empacotador suporta builds Linux locais com glibc.' ;; esac
for pomni_binary in pomni pomni-gui; do
    [[ -f "target/release/$pomni_binary" ]] || fail 'Compile primeiro: cargo build --release --locked --features gui --bins'
done
[[ -z "$(git status --porcelain)" ]] || fail 'Faça commit das alterações antes de gerar o pacote da release.'

pomni_temporary=$(mktemp -d -t pomni-package.XXXXXXXX)
pomni_archive_temporary=''
cleanup() {
    rm -rf -- "$pomni_temporary"
    [[ -z "$pomni_archive_temporary" ]] || rm -f -- "$pomni_archive_temporary"
}
trap cleanup EXIT
cargo metadata --offline --locked --features gui --filter-platform "$pomni_host" --format-version 1 > "$pomni_temporary/metadata.json"
pomni_version=$(jq -er '.resolve.root as $root | .packages[] | select(.id == $root) | .version' "$pomni_temporary/metadata.json")
pomni_name="pomni-$pomni_version-linux-$(uname -m)"
pomni_stage="$pomni_temporary/$pomni_name"
mkdir -p -- "$pomni_stage/licenses"
for pomni_source in target/release/pomni target/release/pomni-gui README.md LICENSE; do
    cp -p -- "$pomni_source" "$pomni_stage/"
done
cp -a -- udev "$pomni_stage/udev"
for pomni_asset in pomni.desktop pomni.svg pomni.png; do
    cp -p -- "assets/$pomni_asset" "$pomni_stage/"
done
cat > "$pomni_stage/THIRD_PARTY_NOTICES.md" <<'NOTICES'
# Third-party software notices

Pomni's MIT license applies to its own code. Dependencies retain their licenses.
Full notices and font licenses are included in licenses/.

NOTICES
jq -r '.packages | sort_by(.name, .version) | .[] | select(.name != "pomni") |
    [.name, .version, .manifest_path, (.license // "see included license file"),
    (.repository // .homepage // ("https://crates.io/crates/" + .name)),
    (if (.authors | length) > 0 then (.authors | join(", ")) else "see included notices" end)] | @tsv' \
    "$pomni_temporary/metadata.json" > "$pomni_temporary/packages.tsv"
while IFS=$'\t' read -r pomni_crate_name pomni_crate_version pomni_manifest pomni_license_name pomni_repository pomni_authors; do
    pomni_crate=$(dirname -- "$pomni_manifest")
    [[ -d "$pomni_crate" ]] || fail "Dependência não encontrada: $pomni_crate_name"
    pomni_destination="$pomni_stage/licenses/$pomni_crate_name-$pomni_crate_version"
    pomni_found=false
    while IFS= read -r -d '' pomni_license; do
        pomni_relative=${pomni_license#"$pomni_crate"/}
        install -Dm644 -- "$pomni_license" "$pomni_destination/$pomni_relative"
        pomni_found=true
    done < <(find "$pomni_crate" -type f ! -name '*.rs' \( \
        -iname '*license*' -o -iname '*copying*' -o -iname '*notice*' -o \
        -iname '*copyright*' -o -iname '*ofl.txt' -o -iname '*ufl.txt' \) -print0)
    pomni_group=''
    case "$pomni_repository" in *github.com/emilk/egui*) pomni_group=egui ;; esac
    if [[ -z "$pomni_group" ]]; then
        case "$pomni_crate_name" in
            accesskit) pomni_group=accesskit ;;
            gl_generator|khronos_api) pomni_group=gl-rs ;;
            profiling) pomni_group=profiling ;;
        esac
    fi
    if [[ -n "$pomni_group" ]]; then
        mkdir -p -- "$pomni_destination"
        cp -a -- "assets/licenses/$pomni_group" "$pomni_destination/upstream"
        pomni_found=true
    fi
    [[ "$pomni_found" == true ]] || fail "Licença não localizada para $pomni_crate_name"
    printf '## %s %s\nLicense: %s\nAuthors: %s\nSource: %s\n\n' \
        "$pomni_crate_name" "$pomni_crate_version" "$pomni_license_name" "$pomni_authors" "$pomni_repository" \
        >> "$pomni_stage/THIRD_PARTY_NOTICES.md"
done < "$pomni_temporary/packages.tsv"
cp -p -- assets/licenses/sources.json "$pomni_stage/licenses/upstream-sources.json"
objdump -T target/release/pomni-gui > "$pomni_temporary/symbols.txt"
pomni_glibc=$(grep -oE 'GLIBC_[0-9]+(\.[0-9]+)+' "$pomni_temporary/symbols.txt" | sed 's/GLIBC_//' | sort -Vu | tail -n1)
[[ -n "$pomni_glibc" ]] || fail 'Não foi possível identificar a versão mínima da glibc.'
{
    printf 'Pomni %s\nCommit: %s\nTarget: %s\nRust: %s\n' \
        "$pomni_version" "$(git rev-parse HEAD)" "$pomni_host" "$(rustc --version)"
    printf 'Minimum GLIBC symbol version: %s\n' "$pomni_glibc"
    printf 'Runtime: Linux %s, OpenGL, Wayland or X11, libgcc_s.so.1\n' "$(uname -m)"
} > "$pomni_stage/BUILD-INFO.txt"
mkdir -p -- dist
pomni_archive_temporary=$(mktemp "$pomni_root/dist/.pomni-release.XXXXXXXX.tar.gz")
tar -czf "$pomni_archive_temporary" -C "$pomni_temporary" "$pomni_name"
mv -f -- "$pomni_archive_temporary" "dist/$pomni_name.tar.gz"
(cd dist && sha256sum "$pomni_name.tar.gz" > SHA256SUMS)
printf '%s\n' "$pomni_root/dist/$pomni_name.tar.gz"
cat "$pomni_stage/BUILD-INFO.txt"
cat dist/SHA256SUMS

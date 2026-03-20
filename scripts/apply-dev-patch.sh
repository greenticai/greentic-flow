#!/usr/bin/env bash
set -euo pipefail

# Скрипт для локального применения патчей из ci/cargo-patch-dev.toml
# Позволяет протестировать сборку с dev-зависимостями из CodeArtifact

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"
PATCH_FILE="${REPO_ROOT}/ci/cargo-patch-dev.toml"
CARGO_TOML="${REPO_ROOT}/Cargo.toml"

if [ ! -f "${PATCH_FILE}" ]; then
    echo "Ошибка: Файл патча не найден: ${PATCH_FILE}"
    exit 1
fi

echo "Применение патчей из ${PATCH_FILE} к ${CARGO_TOML}..."

# Проверка, не применен ли патч уже
if grep -q "\[patch.crates-io\]" "${CARGO_TOML}"; then
    echo "Предупреждение: Секция [patch.crates-io] уже существует в Cargo.toml."
    echo "Убедитесь, что вы не применяете патч дважды."
else
    printf '\n' >> "${CARGO_TOML}"
    cat "${PATCH_FILE}" >> "${CARGO_TOML}"
fi

echo "Фиксация версий (pinning) с использованием 'cargo update --precise'..."

while IFS= read -r line; do
    # Извлечение имени пакета и версии (более надежно)
    name=$(echo "$line" | cut -d' ' -f1)
    version=$(echo "$line" | sed -nE 's/.*version\s*=\s*"=?([^"]+)".*/\1/p')
    
    if [ -n "${name}" ] && [ -n "${version}" ]; then
        echo "  - Фиксация ${name} на версию ${version}"
        cargo update -p "${name}" --precise "${version}" || echo "    Пропущено: ${name} (возможно, не используется или версия не найдена)"
    fi
done < <(grep -E '^[a-zA-Z0-9_-]+\s*=' "${PATCH_FILE}" | grep "version")

echo "Готово. Теперь вы можете запустить 'cargo build' или другие команды."
echo "ВНИМАНИЕ: Не фиксируйте изменения в Cargo.toml и Cargo.lock, если это не требуется."

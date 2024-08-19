USERNAME="iankressin"
REPO_NAME="eql"
EQL_DIR="$HOME/.eql"
EQLUP_URL="https://raw.githubusercontent.com/${USERNAME}/${REPO_NAME}/main/eqlup/eqlup.sh"

initial_message() {
    echo "
    ███████╗ ██████╗ ██╗     
    ██╔════╝██╔═══██╗██║     
    █████╗  ██║   ██║██║     
    ██╔══╝  ██║▄▄ ██║██║     
    ███████╗╚██████╔╝███████╗
    ╚══════╝ ╚══▀▀═╝ ╚══════╝
    "

    echo "[INFO] Installing eqlup, the version manager of EQL"
}

remove_old_version() {
    echo "[INFO] Removing old version of eqlup"
    sudo rm -f "$EQL_DIR/eqlup"
    echo "[INFO] Old version removed "
}

download_eqlup() {
    curl -s -o eqlup.sh $EQLUP_URL
    chmod +x eqlup.sh
}

create_eql_dir_if_needed() {
    if [ ! -d "$EQL_DIR" ]; then
        echo "[INFO] EQL directory does not exist. Creating at: $EQL_DIR"
        mkdir -p "$EQL_DIR"
        echo "[INFO] Directory created successfully."
    else
        echo "[INFO] EQL Directory found. Skipping"
    fi
}

move_eqlup() {
    sudo mv eqlup.sh "$EQL_DIR/eqlup"
}

final_message() {
    echo "---------------------- Installation complete ----------------------"
    echo ">>> Run 'eqlup' to install EVM Query Language (EQL)"
}

main() {
    initial_message
    remove_old_version
    download_eqlup
    create_eql_dir_if_needed
    move_eqlup
    final_message
}

main

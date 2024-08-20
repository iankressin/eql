USERNAME="iankressin"
REPO_NAME="eql"
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
    sudo rm -f /usr/local/bin/eqlup
    echo "[INFO] Old version removed "
}

download_eqlup() {
    curl -s -o eqlup.sh $EQLUP_URL
    chmod +x eqlup.sh
}

move_eqlup() {
    sudo mv eqlup.sh /usr/local/bin/eqlup
}

final_message() {
    echo "---------------------- Installation complete ----------------------"
    echo ">>> Run 'eqlup' to install EVM Query Language (EQL)"
}

main() {
    initial_message
    remove_old_version
    download_eqlup
    move_eqlup
    final_message
}

main

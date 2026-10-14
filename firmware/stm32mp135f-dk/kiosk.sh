#!/bin/sh
# The kiosk globe on the STM32MP135F-DK, from the Mac, over ssh (root, no password on the ST image).
#
#   ./kiosk.sh deploy    build for the board, copy the program and the city image to /usr/local/geodb and
#                        the `globe` command (start / stop on the board itself) to /usr/local/bin
#   ./kiosk.sh start [flags]  stop Weston (and ST's demo launcher with it), start the globe full screen
#                        (flags go to geodb-kiosk, e.g. --quick for the F769's coarse moving globe)
#   ./kiosk.sh stop      stop the globe, start Weston again
#   ./kiosk.sh log       what the globe printed (frame times)
#   ./kiosk.sh shot      the screen as it is into screen.png, here
#   ./kiosk.sh preview   one frame into preview.png, on this machine
#
# BOARD=<address> picks the board (default stm32mp1.local); KNOWN_HOSTS=<file> keeps its host key apart.
set -e
cd "$(dirname "$0")"
BOARD=${BOARD:-stm32mp1.local}
TARGET=armv7-unknown-linux-musleabihf
DIR=/usr/local/geodb
IMAGE=../stm32f769i-disco/geodb.fw
OPTS="-o ConnectTimeout=40 -o StrictHostKeyChecking=accept-new -o LogLevel=ERROR"
[ -n "$KNOWN_HOSTS" ] && OPTS="$OPTS -o UserKnownHostsFile=$KNOWN_HOSTS"

on_board() { ssh $OPTS "root@$BOARD" "$@"; }

case "$1" in
deploy)
    cargo build --release --target $TARGET
    on_board "mkdir -p $DIR"
    # (a running kiosk keeps its binary busy: copy beside it, then rename)
    scp -q $OPTS target/$TARGET/release/geodb-kiosk "root@$BOARD:$DIR/geodb-kiosk.new"
    scp -q $OPTS "$IMAGE" "root@$BOARD:$DIR/geodb.fw"
    scp -q $OPTS globe "root@$BOARD:/usr/local/bin/globe"
    on_board "mv $DIR/geodb-kiosk.new $DIR/geodb-kiosk && ls -la $DIR/geodb-kiosk $DIR/geodb.fw"
    ;;
start)
    shift
    on_board "globe start $*; sleep 3; cat /tmp/kiosk.log"
    ;;
stop)
    on_board "globe stop"
    ;;
log)
    on_board "cat /tmp/kiosk.log"
    ;;
shot)
    on_board "rm -f /tmp/kiosk.png; touch /tmp/kiosk-shot; for i in 1 2 3 4 5 6 7 8 9 10; do
        [ -f /tmp/kiosk.png ] && break; sleep 0.3; done"
    scp -q $OPTS "root@$BOARD:/tmp/kiosk.png" screen.png && echo "saved screen.png"
    ;;
preview)
    cargo run --release -- "$IMAGE" --png preview.png
    ;;
*)
    sed -n '2,14p' "$0"
    exit 2
    ;;
esac

; Helpers for the demo scripts (loaded with (load "LIB.LSP")).
; The board functions come from the mirror (type (help) in its console): (view lat lon zoom) (spin T|nil dps)
; (layers bits) (tap x y) (sleep ms) (fly lat lon zoom ms) (hop "name" zoom ms) (goto "name" zoom) (city "name") ...
; The video clock only advances in (sleep) and in the moves (fly, hop). The screen shows the line that runs.

(defun hold (ms) (sleep ms))

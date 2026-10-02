; Helpers for the demo scripts (loaded with (load "LIB.LSP")).
; The board functions come from the mirror: (view lat lon zoom) (spin T|nil dps) (layers bits) (tap x y)
; (sleep ms) (lat) (lon) (zoom) and (city "name"); the video clock only advances in (sleep).

; Smooth move from the current view to la lo z in ms: the zoom changes geometrically, the
; longitude takes the short way round, an ease in and out.
(defun fly (la lo z ms / n i la0 lo0 z0 dlo u s)
  (setq n (max 1 (fix (/ ms 40))) i 0 la0 (lat) lo0 (lon) z0 (zoom))
  (setq dlo (- lo lo0))
  (if (> dlo 180) (setq dlo (- dlo 360)))
  (if (< dlo -180) (setq dlo (+ dlo 360)))
  (while (< i n)
    (setq i (1+ i)
          u (/ (float i) n)
          s (* u u (- 3 (* 2 u))))
    (view (+ la0 (* s (- la la0))) (+ lo0 (* s dlo)) (* z0 (expt (/ z z0) s)))
    (sleep 40))
  T)

; A trip to a place by name: out to the whole-globe view, across, in to zoom z. Returns the name.
(defun hop (name z ms / c)
  (setq c (city name))
  (if c
    (progn
      (fly (lat) (lon) 1.3 (* ms 0.25))
      (fly (nth 0 c) (nth 1 c) 1.3 (* ms 0.35))
      (fly (nth 0 c) (nth 1 c) z (* ms 0.4))
      (nth 2 c))
    (progn (princ (strcat "no place " name)) nil)))

(defun hold (ms) (sleep ms))

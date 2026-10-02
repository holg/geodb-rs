; The LAYERS button, pressed on the touch screen (taps, as a finger): lights the layers one by one
; and, once all are lit, switches them off one by one.
(load "LIB.LSP")
(layers 0)
(view 40 10 1)
(spin T 12)
(hold 1500)
(repeat 10          ; 5 up, 5 down
  (tap 640 454)     ; the LAYERS button
  (hold 1800))
(spin nil)

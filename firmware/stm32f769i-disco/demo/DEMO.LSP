; The demo video: the globe from the whole earth to a mountain, layer by layer, and a trip round it.
;   cargo run --release -p geodb-fw-core --features lisp --example mirror -- --headless firmware/stm32f769i-disco/demo/DEMO.LSP --out demo.mp4
(load "LIB.LSP")

; 1. the bare earth, turning (natural direction)
(layers 0)
(view 25 10 1)
(spin T 18)
(hold 4000)

; 2. the layers, one after the other (what the LAYERS button does): coast, relief, isohypses, dots
(layers 4)  (hold 2000)
(layers 5)  (hold 2500)
(layers 7)  (hold 2500)
(layers 15) (hold 1500)
(layers 95) (hold 1500)

; 3. stop, and in over the Alps to a summit: the relief and the isohypses over the Alps
(spin nil)
(hop "Munich" 6 3500)
(hold 1500)
(fly 47.3 10.7 9 3500)           ; the Alps: relief and isohypses on the earth (zoom up to 11)
(hold 3000)
(layers 5)  (hold 2000)          ; relief only
(layers 7)  (hold 2000)          ; + isohypses
(layers 95) (hold 1000)
(fly 47.0 10.5 22 3500)          ; zoom 12 and up is the scope view: the cities around the point
(hold 3500)

; 4. a trip round the world
(foreach place '("Tokyo" "Sydney" "Cape Town" "Rio de Janeiro" "New York")
  (hop place 5 5000)
  (hold 1800))

; 5. back out, spinning the other way round
(fly 20 -30 1 4000)
(spin T -22)
(hold 6000)
(layers 4) (hold 2000)
(layers 95) (hold 3000)
(spin nil)

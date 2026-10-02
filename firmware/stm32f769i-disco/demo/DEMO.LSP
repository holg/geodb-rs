; The demo video: the globe from the whole earth to a mountain, layer by layer, and a trip round it.
; (note "...") is the yellow caption on the screen, the green line under it is the code that runs.
;   cargo run --release -p geodb-mirror -- --headless firmware/stm32f769i-disco/demo/DEMO.LSP --out demo.mp4
(load "LIB.LSP")

; 1. the bare earth, turning
(note "A pocket globe: 40928 cities")
(layers 0)
(view 25 10 1)
(spin T 18)
(hold 4000)

; 2. the layers, one after the other (what the LAYERS button does)
(note "Layer 1: the coastline")
(layers 4)  (hold 2500)
(note "Layer 2: relief shading")
(layers 5)  (hold 3000)
(note "Layer 3: isohypses")
(layers 7)  (hold 3000)
(note "Layer 4: every city a dot")
(layers 15) (hold 2500)
(note "Layer 5: the query ring")
(layers 95) (hold 2000)

; 3. stop, and in over the Alps
(spin nil)
(note "Stop, fly to Munich")
(hop "Munich" 6 3500)
(hold 1500)
(note "The Alps: relief, isohypses")
(fly 47.3 10.7 9 3500)
(hold 3000)
(note "Relief only")
(layers 5)  (hold 2000)
(note "Relief and isohypses")
(layers 7)  (hold 2000)
(note "All layers again")
(layers 95) (hold 1000)
(note "Zoom 12+: the scope view")
(fly 47.0 10.5 22 3500)
(hold 3500)

; 4. a trip round the world
(foreach place '("Tokyo" "Sydney" "Cape Town" "Rio de Janeiro" "New York")
  (note (strcat "Trip to " place))
  (hop place 5 5000)
  (hold 1800))

; 5. back out, spinning the other way round
(note "Out to the whole globe")
(fly 20 -30 1 4000)
(note "Spinning the other way")
(spin T -22)
(hold 6000)
(note "Layers on a spinning globe")
(layers 4) (hold 2000)
(layers 95) (hold 3000)
(spin nil)

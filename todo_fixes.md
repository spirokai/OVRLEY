[MORE COMPLICATED]
L. Add a switch to route/elevation widget styling in . By default both will only construct polyline and show progression for the custom exported range, e.g. if range is 600 to 630 seconds, only the part of route/elevation that corresponds to this will be rendered. If switched on, the entire route/elevation is rendered regardless of the custom export range (the current state). This should also be reflected in the preview. That also means we have to also copy the custom range bar from RenderVideoDialog to SidebarSettings, right underneath the widget update rate.
I. fix gradient widget, including the triangle indicator
J. parity of map/elevation - bring skia render into preview?

A. moveable lines and control points too thin and small in Tauri, they are ok in browser - how do we fix that?
B. add system fonts, separate group from "recommended fonts", recommended fonts should be at the top of the list, separated from others with a horizontal line
C. remove horizontal lines and the extra vertical spacing from widget accordion list; keep grouping by widget type
D. add the rendering options (fps, widget update rate, export range, codec) also to the render modal -> after pressing the render button, the modal opens, user confirms the render settings; and only then the rendering starts in the same modal (the current behavior). The codec and custom range do not affect the preview, because of that they should be removed from the sidebar, only kept in the modal. FPS and widget update rate does affect both, so they should remain in the sidebar as well as in the modal.
E. create a stylized color picker from radix primitives that fits the rest of the ui styling, without alpha channel setting (we have opacity slider for that) and use it for all color choosers in the app. Verify if we can perhapse reause/customize this one: https://www.shadcn.io/components/color-picker
F. add a button for a new template in the header next to the others; this will reset the editor canvas to default
G. make ui responsive/scale by using dynamic font-size in body/root based on window width, with a minimum and maximum font sizes (use clamp); can we add minimum window size? Can we ensure it opens maximized?

[MORE COMPLICATED]
H. after we changed the frontend preview text in widgets to svg (to maintain parity with skia renderer), the shadows are not applied in the preview
L. add a switch to route/elevation widget styling. By default both will only construct polyline and show progression for the custom exported range, e.g. if range is 600 to 630 seconds, only the part of route/elevation that corresponds to this will be rendered. If switched on, the entire route/elevation is rendered regardless of the custom export range (the current state)
I. fix gradient widget, including the triangle indicator
J. parity of map/elevation - bring skia render into preview?
K. global scaling interacting with resizing

[LATER POLISH]
M. polish the widget stylizing options, especially aimed at clear naming and organization.
N. Do we need 8 useEffect here? Sounds kinda bonkers.

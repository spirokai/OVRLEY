-after changing preview text to svg, the shadows are not applied in the preview
-moveable lines and control points too thin and small in Tauri, they are ok in browser - how do we fix that?
-add system fonts, separate group from "recommended fonts", recommended fonts should be at the top of the list, separated from others with a horizontal line
-remove horizontal lines from widget list
-move rendering options to the render modal -> after pressing the render button, the modal opens, user confirms the render settings and only then the rendering starts
-create a stylized color picker from radix primitives that fits the rest of the ui styling, without alpha channel setting (we have opacity slider for that)
-polish the widget stylizing options, especially aimed at clear naming and organization
-add a button for a new template in the header next to the others
-make ui responsive/scale, use dynamic font-size in body/root to scale the ui; can we add minimum window size? Can we make it open maximized?

[MORE COMPLICATED]
-fix gradient widget, including the triangle indicator
-parity of map/elevation - bring skia render into preview?
-global scaling interacting with resizing

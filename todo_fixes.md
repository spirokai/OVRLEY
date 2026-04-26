-after changing preview text to svg, the shadows are not applied in the preview
-moveable lines and control points too thin and small in Tauri, they are ok in browser - how do we fix that?
-add system fonts, separate group from "recommended fonts", recommended fonts should be at the top of the list, separated from others with a horizontal line
-remove horizontal lines from widget list
-add the rendering options also to the render modal -> after pressing the render button, the modal opens, user confirms the render settings and only then the rendering starts. The codec and custom range do not affect the preview, because of that they should not be in the sidebar, only in the modal. FPS and widget update rate does affect both, so they should be in the sidebar as well as in the modal.
-create a stylized color picker from radix primitives that fits the rest of the ui styling, without alpha channel setting (we have opacity slider for that) and use it for all color choosers in the app
-add a button for a new template in the header next to the others
-make ui responsive/scale, use dynamic font-size in body/root based on window width, with a minimum font size (use clamp); can we add minimum window size? Can we ensure it opens maximized?
-polish the widget stylizing options, especially aimed at clear naming and organization

[MORE COMPLICATED]
-fix gradient widget, including the triangle indicator
-parity of map/elevation - bring skia render into preview?
-global scaling interacting with resizing

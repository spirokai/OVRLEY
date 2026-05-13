/**
 * Renders the control panel portion of the application interface.
 */

import { Settings2, Activity } from 'lucide-react'
import { Tabs, TabsContent, TabsList, TabsTrigger } from '@/components/ui/tabs'
import { SidebarSettingsTab } from '@/features/scene-settings'
import SidebarWidgetsTab from './SidebarWidgetsTab'

/**
 * Renders the control panel component.
 *
 * @param {object} props - Component props.
 * @param {*} props.config - Overlay template configuration data.
 * @param {*} props.onConfigChange - Callback invoked to config change.
 * @returns {JSX.Element} Rendered component output.
 */
export default function ControlPanel({ config, onConfigChange }) {
  return (
    <div className="flex flex-col h-full bg-card/10">
      <Tabs defaultValue="settings" className="flex-1 flex flex-col min-h-0">
        <div className="px-4 pt-4 shrink-0">
          <TabsList className="grid w-full grid-cols-2 bg-surface p-1">
            <TabsTrigger value="settings" className="text-xs gap-2 cursor-pointer">
              <Settings2 className="h-3 w-3" />
              Settings
            </TabsTrigger>
            <TabsTrigger value="widgets" className="text-xs gap-2 cursor-pointer">
              <Activity className="h-3 w-3" />
              Widgets
            </TabsTrigger>
          </TabsList>
        </div>

        <div className="flex-1 overflow-y-auto px-4 pb-4 min-h-0 [scrollbar-gutter:stable]">
          <TabsContent value="settings" className="outline-none">
            <SidebarSettingsTab config={config} onConfigChange={onConfigChange} />
          </TabsContent>

          <TabsContent value="widgets" className="mt-4 outline-none">
            <SidebarWidgetsTab />
          </TabsContent>
        </div>
      </Tabs>
    </div>
  )
}

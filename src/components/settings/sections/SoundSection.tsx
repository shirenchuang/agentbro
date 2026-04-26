import { useTranslation } from 'react-i18next'
import { useConfigStore } from '../../../stores/configStore'
import { setSoundVolume, setSoundEnabled } from '../../../services/tauriApi'
import { SettingSection } from '../SettingSection'
import { SettingGroup } from '../SettingGroup'
import { SettingRow } from '../SettingRow'
import { Toggle } from '../Toggle'
import { Slider } from '../Slider'
import { Dropdown } from '../Dropdown'

let sharedAudioCtx: AudioContext | null = null

function playPreviewBeep(volume: number) {
  try {
    if (!sharedAudioCtx || sharedAudioCtx.state === 'closed') {
      sharedAudioCtx = new AudioContext()
    }
    const ctx = sharedAudioCtx
    // Resume if suspended (browser autoplay policy)
    if (ctx.state === 'suspended') {
      ctx.resume()
    }
    const osc = ctx.createOscillator()
    const gain = ctx.createGain()
    osc.connect(gain)
    gain.connect(ctx.destination)
    osc.frequency.value = 800
    osc.type = 'square'
    gain.gain.value = (volume / 100) * 0.15
    gain.gain.exponentialRampToValueAtTime(0.001, ctx.currentTime + 0.15)
    osc.start()
    osc.stop(ctx.currentTime + 0.15)
  } catch {
    // Web Audio not available
  }
}

function PlayButton({ volume, title }: { volume: number; title: string }) {
  return (
    <button
      className="sound-event-row__play"
      onClick={() => playPreviewBeep(volume)}
      title={title}
    >
      ▶
    </button>
  )
}

export function SoundSection() {
  const { t } = useTranslation()
  const config = useConfigStore()
  const sessionEvents = config.soundEvents.filter((e) => e.group === 'session')
  const interactionEvents = config.soundEvents.filter((e) => e.group === 'interaction')
  const systemEvents = config.soundEvents.filter((e) => e.group === 'system')

  const soundPackOptions = [
    { value: 'eight-bit', label: t('settings.eightBitRetro') },
    { value: 'subtle', label: t('settings.subtle') },
    { value: 'custom', label: t('settings.custom') },
  ]

  return (
    <SettingSection title={t('settings.sound')} description={t('settings.soundDesc')}>
      <SettingGroup>
        <SettingRow label={t('settings.enableSounds')} description={t('settings.enableSoundsDesc')}>
          <Toggle checked={config.soundEnabled} onChange={(v) => { config.updateConfig('soundEnabled', v); setSoundEnabled(v) }} />
        </SettingRow>
        <SettingRow label={t('settings.volume')}>
          <Slider
            value={config.volume}
            min={0}
            max={100}
            onChange={(v) => { config.updateConfig('volume', v); setSoundVolume(v) }}
            unit="%"
          />
        </SettingRow>
        <SettingRow label={t('settings.soundPack')} description={t('settings.soundPackDesc')}>
          <Dropdown
            value={config.soundPack}
            options={soundPackOptions}
            onChange={(v) => config.updateConfig('soundPack', v as 'eight-bit' | 'subtle' | 'custom')}
            minWidth={130}
          />
        </SettingRow>
      </SettingGroup>

      <SettingGroup label={t('settings.sessionEvents')}>
        {sessionEvents.map((event) => (
          <div key={event.id} className="sound-event-row">
            <span className="sound-event-row__label">{event.label}</span>
            <PlayButton volume={config.volume} title={t('settings.previewSound')} />
            <Toggle
              checked={event.enabled}
              onChange={() => config.toggleSoundEvent(event.id)}
              disabled={!config.soundEnabled}
            />
          </div>
        ))}
      </SettingGroup>

      <SettingGroup label={t('settings.interactionEvents')}>
        {interactionEvents.map((event) => (
          <div key={event.id} className="sound-event-row">
            <span className="sound-event-row__label">{event.label}</span>
            <PlayButton volume={config.volume} title={t('settings.previewSound')} />
            <Toggle
              checked={event.enabled}
              onChange={() => config.toggleSoundEvent(event.id)}
              disabled={!config.soundEnabled}
            />
          </div>
        ))}
      </SettingGroup>

      <SettingGroup label={t('settings.systemEvents')}>
        {systemEvents.map((event) => (
          <div key={event.id} className="sound-event-row">
            <span className="sound-event-row__label">{event.label}</span>
            <PlayButton volume={config.volume} title={t('settings.previewSound')} />
            <Toggle
              checked={event.enabled}
              onChange={() => config.toggleSoundEvent(event.id)}
              disabled={!config.soundEnabled}
            />
          </div>
        ))}
      </SettingGroup>

      <SettingGroup>
        <SettingRow label={t('settings.probeFilter')} description={t('settings.probeFilterDesc')}>
          <Toggle checked={config.probeSessionFilter} onChange={(v) => config.updateConfig('probeSessionFilter', v)} />
        </SettingRow>
      </SettingGroup>
    </SettingSection>
  )
}

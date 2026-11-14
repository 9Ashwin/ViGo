import { Button, Dropdown, FluentProvider, Input, Option, Tab, TabList } from '@fluentui/react-components';
import { Apps24Regular, Cursor24Regular, DataPie24Regular, Settings24Regular, Add20Regular, Dismiss20Regular, Play20Regular, Flash24Regular } from '@fluentui/react-icons';
import { useEffect, useMemo, useRef, useState } from 'react';
import { DEFAULT_SETTINGS, triggerLabel, type Settings } from '../../shared/settings';
import type { InstalledApp, SettingsState } from '../../preload/settings';
import { api } from './api';
import { macDarkTheme, macLightTheme } from './macTheme';
import { SettingRow, SliderRow, SwitchRow } from './components/rows';
import { TriggerPicker } from './components/TriggerPicker';
import { WheelEditor } from './components/WheelEditor';
import logo from '../assets/vigo.svg';
import mascot from '../assets/mascot.png';
import { fallbackIcon } from '../../shared/icons';


const SECTIONS = [
  { id: 'wheel', label: '我的轮盘', icon: <DataPie24Regular /> },
  { id: 'trigger', label: '呼出与手势', icon: <Cursor24Regular /> },
  { id: 'app', label: '通用设置', icon: <Settings24Regular /> },
] as const;

function useDarkMode(): boolean {
  const [dark, setDark] = useState(() => matchMedia('(prefers-color-scheme: dark)').matches);
  useEffect(() => {
    const m = matchMedia('(prefers-color-scheme: dark)');
    const onChange = () => setDark(m.matches);
    m.addEventListener('change', onChange);
    return () => m.removeEventListener('change', onChange);
  }, []);
  return dark;
}

export function App() {
  const systemDark = useDarkMode();
  const [state, setState] = useState<SettingsState>();
  const [section, setSection] = useState('wheel');
  const [page, setPage] = useState(0);
  const [filter, setFilter] = useState('');
  const [category, setCategory] = useState('apps');
  const [shortcuts, setShortcuts] = useState<InstalledApp[]>([]);
  const [icons, setIcons] = useState<Record<string, string>>({});
  const [error, setError] = useState('');
  const revision = useRef(0);
  const loadedIcons = useRef(new Set<string>());
  useEffect(() => {
    void api.get().then(setState).catch((e: Error) => setError(e.message));
    return api.onChange((next) => setState((old) => ({ ...old, ...next, installed: next.installed ?? old?.installed })));
  }, []);
  useEffect(() => { if (category === 'shortcuts') void api.shortcuts().then(setShortcuts); }, [category]);

  const s = state?.settings ?? DEFAULT_SETTINGS;
  const dark = s.appearance === 'system' ? systemDark : s.appearance === 'dark';
  useEffect(() => { document.documentElement.dataset.theme = dark ? 'dark' : 'light'; }, [dark]);
  const chosen = s.wheel.filter(Boolean);
  const catalog = useMemo(() => {
    const source = category === 'shortcuts' ? shortcuts : state?.installed ?? [];
    return source.filter((a) => a.name.toLocaleLowerCase().includes(filter.toLocaleLowerCase())).slice(0, 48);
  }, [category, shortcuts, state?.installed, filter]);
  useEffect(() => {
    let cancelled = false;
    const paths = [...new Set([...catalog.map((a) => a.path), ...chosen])]
      .filter((path) => path && !path.startsWith('shortcuts://') && !loadedIcons.current.has(path));
    let index = 0;
    const worker = async () => {
      while (!cancelled && index < paths.length) {
        const path = paths[index++];
        loadedIcons.current.add(path);
        const icon = await api.icon(path).catch(() => '');
        if (!cancelled && icon) setIcons((old) => ({ ...old, [path]: icon }));
        if (cancelled) loadedIcons.current.delete(path);
      }
    };
    void Promise.all(Array.from({ length: 4 }, worker));
    return () => { cancelled = true; };
  }, [catalog, s.wheel]);

  async function update(patch: Partial<Settings>) {
    const id = ++revision.current;
    const next = { ...s, ...patch };
    setState((old) => old ? { ...old, settings: next } : old);
    try {
      const saved = await api.save(next);
      if (id === revision.current) { setState((old) => ({ ...old, ...saved, installed: old?.installed })); setError(''); }
    } catch (e) { setError(String(e)); }
  }
  const nameOf = (path: string): string => s.names[path] || (path.startsWith('shortcuts://')
    ? new URL(path).searchParams.get('name') ?? '快捷指令'
    : path.split('/').pop()?.replace(/\.app$/, '') ?? path);
  const iconOf = (path: string): string => icons[path] || fallbackIcon(path.startsWith('shortcuts://'));
  async function add(paths: string[]) {
    const wheel = [...s.wheel];
    for (const path of paths) {
      if (wheel.includes(path)) continue;
      let at = wheel.indexOf('');
      if (at < 0 && wheel.length < 24) { at = wheel.length; wheel.push(...Array<string>(8).fill('')); }
      if (at >= 0) wheel[at] = path;
      else { setError('轮盘最多可以固定 24 项'); break; }
    }
    await update({ wheel });
  }
  const pageWheel = Array.from({ length: 8 }, (_, i) => s.wheel[page * 8 + i] ?? '');
  const pool = [...new Map([...catalog, ...chosen.map((path) => ({ path, name: nameOf(path) }))].map((a) => [a.path, a])).values()]
    .map((a) => ({ id: a.path, name: s.names[a.path] || a.name, icon: iconOf(a.path),
      onRemove: chosen.includes(a.path) ? () => { void update({ wheel: s.wheel.map((p) => p === a.path ? '' : p) }); } : undefined }));
  const trigger = triggerLabel(s.trigger);
  const title = SECTIONS.find((item) => item.id === section)?.label;
  const running = state?.status === 'running';

  return <FluentProvider theme={dark ? macDarkTheme : macLightTheme} style={{ background: 'transparent', height: '100%' }}>
    <div className="app">
      <header className="titlebar">设置 <span style={{ opacity: .4, margin: '0 4px' }}>/</span> {title}</header>
      <nav className="side" aria-label="设置导航">
        <div className="brand"><img src={logo} alt="ViGo" /><div><b>ViGo</b><span className="desc">V for your desktop.</span></div></div>
        {SECTIONS.map((item) => <button key={item.id} className={`navItem${section === item.id ? ' sel' : ''}`} onClick={() => setSection(item.id)}>{item.icon}{item.label}</button>)}
        <div className="sidebarFoot"><span className={`connectionDot ${running ? 'ready' : ''}`} />{running ? '输入助手运行中' : '等待辅助功能权限'}<small>以 LoL ping 的方式，操作你的桌面。</small></div>
      </nav>
      <main className="content">
        {error && <div className="inlineError" role="alert">{error}<Button size="small" appearance="transparent" icon={<Dismiss20Regular />} onClick={() => setError('')} /></div>}
        {!state ? <div className="card">正在加载…</div> : <>
          <div className="card status"><img className="statusPing" src={logo} alt="" /><div className="lbl"><b style={{ fontSize: 17 }}>{running ? (s.enabledOnStart ? 'ViGo 已就绪' : 'ViGo 已暂停') : '允许 ViGo 读取呼出按键'}</b><span className="desc">{running ? `按住 ${trigger}，移动选择，松开执行。` : '授予一次辅助功能权限后，输入助手会自动连接。'}</span></div><Button icon={<Play20Regular />} onClick={() => void api.previewWheel(page)}>试一下</Button></div>
          {state.status === 'noAccess' && <div className="card permission"><div className="lbl"><b>{state.staleAccess ? '系统保留了一个失效授权条目' : '只需要辅助功能权限'}</b><span className="desc">{state.staleAccess ? '在系统辅助功能中移除旧条目，再添加 /Applications/ViGo.app。' : '用于识别 V 键和鼠标手势；ViGo 会在你开启权限后自动恢复。'}</span></div><Button appearance="primary" onClick={() => void api.requestAccessibility()}>打开系统设置</Button></div>}
          {section === 'wheel' && <>
            <div className="section sectionToolbar"><span>常用动作 <small>{chosen.length} / 24</small></span><div><Button icon={<Add20Regular />} onClick={() => void api.browse().then(add)}>添加文件或文件夹</Button></div></div>
            <div className="catalogToolbar"><TabList selectedValue={category} onTabSelect={(_, d) => { setCategory(String(d.value)); setFilter(''); }}><Tab value="apps" icon={<Apps24Regular />}>软件</Tab><Tab value="shortcuts" icon={<Flash24Regular />}>快捷指令</Tab></TabList><Input type="search" value={filter} onChange={(_, d) => setFilter(d.value)} placeholder={category === 'apps' ? '搜索已安装的软件' : '搜索快捷指令'} /></div>
            <div className="pageTabs">{[0, 1, 2].map((n) => <Button key={n} size="small" appearance={page === n ? 'primary' : 'subtle'} onClick={() => setPage(n)}>轮盘 {n + 1}</Button>)}</div>
            <WheelEditor wheel={pageWheel} center="" centerIcon={mascot} centerOn={false} trigger={trigger} pool={pool} visibleIds={new Set(catalog.map((item) => item.path))} defaults={{ wheel: Array<string>(8).fill(''), center: '' }}
              labels={{ title: '把动作放到顺手的位置', desc: '拖入、交换位置；按住 V 时，滚轮可以切换轮盘。', poolTitle: category === 'apps' ? '已安装的软件' : '你的快捷指令', poolDesc: '拖到左侧轮盘，或点击选中后再点一个槽位。' }}
              poolExtra={catalog.length === 0 ? <span className="desc">没有匹配的动作；可以换个关键词，或添加文件。</span> : undefined}
              onDropFiles={(files) => void add(api.filePaths(files))} onPreview={() => undefined}
              onChange={(patch) => { if (!patch.wheel) return; const wheel = [...s.wheel]; while (wheel.length < (page + 1) * 8) wheel.push(''); const incoming = patch.wheel; for (const value of incoming) if (value) { const old = wheel.indexOf(value); if (old >= 0 && (old < page * 8 || old >= (page + 1) * 8)) wheel[old] = ''; } wheel.splice(page * 8, 8, ...incoming); void update({ wheel }); }} />
            {chosen.length > 0 && <><div className="section">已固定</div><div className="pinnedGroup">{chosen.map((path) => <div className="pinRow" key={path}><img src={iconOf(path)} alt="" /><Input aria-label={`重命名 ${nameOf(path)}`} defaultValue={nameOf(path)} onBlur={(e) => { if (e.currentTarget.value !== nameOf(path)) void update({ names: { ...s.names, [path]: e.currentTarget.value } }); }} /><span className="desc">{path.startsWith('shortcuts://') ? '快捷指令' : path.endsWith('.app') ? '软件' : '文件或文件夹'}</span><Button appearance="subtle" icon={<Dismiss20Regular />} aria-label={`移除 ${nameOf(path)}`} onClick={() => void update({ wheel: s.wheel.map((p) => p === path ? '' : p) })} /></div>)}</div></>}
          </>}
          {section === 'trigger' && <>
            <div className="section">呼出方式</div>
            <SettingRow icon={<Cursor24Regular />} title="轮盘呼出键" description="默认 V。按住后移动到一个方向，松开就执行。"><TriggerPicker value={s.trigger} onChange={(key) => { if (key !== 'off') void update({ trigger: key }); }} /></SettingRow>
            <SettingRow icon={<DataPie24Regular />} title={typeof s.trigger === 'object' ? '轻按仍然输入' : '侧键手势'} description={typeof s.trigger === 'object' ? `轻按 ${trigger} 正常输入，组合键照常使用。` : '启用时侧键用于轮盘；暂停 ViGo 后恢复原功能。'}><span className="desc">{typeof s.trigger === 'object' ? '始终开启' : '按住选择'}</span></SettingRow>
            <SettingRow icon={<Cursor24Regular />} title="取消选择" description="移回中心再松手，或按 Esc、鼠标右键。"><kbd>esc</kbd></SettingRow>
            <div className="section">轮盘显示</div>
            <SliderRow limit="wheelSizePx" icon={<DataPie24Regular />} title="轮盘大小" description="调整轮盘尺寸，图标和命中区域一起缩放。" value={s.wheelSizePx} format={(v) => `${v} pt`} onChange={(v) => void update({ wheelSizePx: v })} />
          </>}
          {section === 'app' && <>
            <div className="section">通用</div>
            <SettingRow icon={<Settings24Regular />} title="外观" description="为当前桌面选择合适的明暗模式。"><Dropdown selectedOptions={[s.appearance]} value={{system: '跟随系统', light: '浅色', dark: '深色'}[s.appearance]} onOptionSelect={(_, d) => { if (d.optionValue) void update({ appearance: d.optionValue as Settings['appearance'] }); }}><Option value="system">跟随系统</Option><Option value="light">浅色</Option><Option value="dark">深色</Option></Dropdown></SettingRow>
            <SwitchRow icon={<Cursor24Regular />} title="启用 ViGo" description="暂停时，所有输入都交还当前软件。" checked={s.enabledOnStart} onChange={(on) => void update({ enabledOnStart: on })} />
            <SwitchRow icon={<Settings24Regular />} title="登录时启动" description="登录后在菜单栏静静待命。" checked={s.launchAtLogin} onChange={(on) => void update({ launchAtLogin: on })} />
            {state.status === 'failed' && <SettingRow icon={<Settings24Regular />} title="输入助手已停止" description="重试连接，不会重新弹出授权提示。"><Button onClick={() => void api.requestAccessibility()}>重新连接</Button></SettingRow>}
            <div className="section">窗口</div>
            <SwitchRow icon={<DataPie24Regular />} title="窗口预览" description="悬停窗口外环时显示画面，需要一次屏幕录制授权。" checked={s.windowPreviews} onChange={(on) => { void update({ windowPreviews: on }); if (on && !state.screenPermission) void api.requestWindowPreview(); }} />
            {s.windowPreviews && !state.screenPermission && <SettingRow icon={<Settings24Regular />} title="屏幕录制尚未开启" description="窗口列表和切换仍可使用，开启后才显示预览。"><Button onClick={() => void api.requestWindowPreview()}>授予屏幕录制权限</Button></SettingRow>}
            <div className="section">关于</div><div className="card notes"><img className="aboutLogo" src={logo} alt="" /><div className="lbl"><b>ViGo</b><span className="desc">按住 V，把常用动作放到手边。</span><span className="desc">为桌面打造的 V 键动作轮盘。</span></div></div>
          </>}
        </>}
      </main>
    </div>
  </FluentProvider>;
}

import { Button, Dropdown, FluentProvider, Input, Option, Tab, TabList } from '@fluentui/react-components';
import { Apps24Regular, Cursor24Regular, DataPie24Regular, Settings24Regular, Add20Regular, Dismiss20Regular, Play20Regular } from '@fluentui/react-icons';
import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { DEFAULT_SETTINGS, triggerLabel, type Settings } from '../../shared/settings';
import type { SettingsState } from '../../preload/settings';
import { api } from './api';
import { macDarkTheme, macLightTheme } from './macTheme';
import { SettingRow, SliderRow, SwitchRow } from './components/rows';
import { TriggerPicker } from './components/TriggerPicker';
import { OuterEditor } from './components/OuterEditor';
import { WheelEditor } from './components/WheelEditor';
import logo from '../assets/app-icon.png';
import { MascotView } from './components/MascotView';
import { fallbackIcon } from '../../shared/icons';
import { applyPagePatch } from '../../shared/wheelLayout';
import { matchesApp } from '../../shared/appSearch';


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
  const [inspected, setInspected] = useState('');
  const [icons, setIcons] = useState<Record<string, string>>({});
  const [error, setError] = useState('');
  const revision = useRef(0);
  const loadedIcons = useRef(new Set<string>());
  useEffect(() => {
    void api.get().then(setState).catch((e: Error) => setError(e.message));
    return api.onChange((next) => setState((old) => ({ ...old, ...next, installed: next.installed ?? old?.installed })));
  }, []);
  useEffect(() => {
    const refresh = () => { void api.get().then((next) => setState((old) => old ? { ...old, installed: next.installed, screenPermission: next.screenPermission } : next)).catch(() => undefined); };
    window.addEventListener('focus', refresh);
    return () => window.removeEventListener('focus', refresh);
  }, []);

  const s = state?.settings ?? DEFAULT_SETTINGS;
  const dark = s.appearance === 'system' ? systemDark : s.appearance === 'dark';
  useEffect(() => { document.documentElement.dataset.theme = dark ? 'dark' : 'light'; }, [dark]);
  const chosen = s.wheel.filter(Boolean);
  const count = s.slotsPerPage;
  const pages = Math.ceil(24 / count);
  useEffect(() => { setPage((old) => Math.min(old, pages - 1)); }, [pages]);
  const catalog = useMemo(() => {
    const source = state?.installed ?? [];
    return source.filter((a) => matchesApp(a, filter));
  }, [state?.installed, filter]);
  const iconQueue = useRef<string[]>([]);
  const iconWorkers = useRef(0);
  const mounted = useRef(true);
  useEffect(() => () => { mounted.current = false; }, []);
  const requestIcon = useCallback((path: string) => {
    if (!path || loadedIcons.current.has(path)) return;
    loadedIcons.current.add(path); iconQueue.current.push(path);
    const pump = () => {
      while (mounted.current && iconWorkers.current < 8 && iconQueue.current.length) {
        const target = iconQueue.current.shift()!; iconWorkers.current++;
        void api.icon(target).then((icon) => { if (mounted.current && icon) setIcons((old) => ({ ...old, [target]: icon })); }).catch(() => undefined)
          .finally(() => { iconWorkers.current--; pump(); });
      }
    };
    pump();
  }, []);
  useEffect(() => { chosen.forEach(requestIcon); }, [s.wheel, requestIcon]);

  async function update(patch: Partial<Settings>) {
    const id = ++revision.current;
    const next = { ...s, ...patch };
    setState((old) => old ? { ...old, settings: next } : old);
    try {
      const saved = await api.save(next);
      if (id === revision.current) { setState((old) => ({ ...old, ...saved, installed: old?.installed })); setError(''); }
    } catch (e) { setError(String(e)); }
  }
  const nameOf = (path: string): string => s.names[path] || state?.installed?.find((a) => a.path === path)?.name || path.split('/').pop()?.replace(/\.app$/, '') || path;
  const iconOf = (path: string): string => icons[path] || fallbackIcon();
  async function add(paths: string[]) {
    const wheel = [...s.wheel];
    for (const path of paths) {
      if (wheel.includes(path)) continue;
      if (wheel.filter(Boolean).length >= 24) { setError('轮盘最多可以固定 24 项'); break; }
      let at = wheel.indexOf('');
      if (at < 0 && wheel.length < pages * count) { at = wheel.length; wheel.push(...Array<string>(count).fill('')); }
      if (at >= 0) wheel[at] = path;
      else { setError('轮盘最多可以固定 24 项'); break; }
    }
    await update({ wheel });
  }
  const pageWheel = Array.from({ length: count }, (_, i) => s.wheel[page * count + i] ?? '');
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
        <div className="sidebarFoot"><span className={`connectionDot ${running && s.enabledOnStart && !state?.capturing ? 'ready' : ''}`} />{state?.capturing ? '正在设置呼出键' : running ? (s.enabledOnStart ? 'ViGo 已就绪' : 'ViGo 已暂停') : '等待辅助功能权限'}<small>按住 V · 移动选择 · 松开执行</small></div>
      </nav>
      <main className="content">
        {error && <div className="inlineError" role="alert">{error}<Button size="small" appearance="transparent" icon={<Dismiss20Regular />} onClick={() => setError('')} /></div>}
        {!state ? <div className="card">正在加载…</div> : <>
          <div className="pageHeading"><div><h1>{title}</h1><p>{section === 'wheel' ? '把软件放进内环，把它的常用内容交给外环。' : section === 'trigger' ? '一次按住，一次移动。让动作顺着你的习惯。' : '轻巧待命，融入你的 Mac。'}</p></div><Button className="previewButton" icon={<Play20Regular />} onClick={() => void api.previewWheel(page)}><kbd>{trigger}</kbd> 试一下</Button></div>
          {state.status === 'noAccess' && <div className="card permission"><div className="lbl"><b>{state.staleAccess ? '系统保留了一个失效授权条目' : '只需要辅助功能权限'}</b><span className="desc">{state.staleAccess ? '在系统辅助功能中移除旧条目，再添加 /Applications/ViGo.app。' : '用于识别 V 键和鼠标手势；ViGo 会在你开启权限后自动恢复。'}</span></div><Button appearance="primary" onClick={() => void api.requestAccessibility()}>打开系统设置</Button></div>}
          {section === 'wheel' && <>
            <div className="section sectionToolbar"><div className="pageTabs">{Array.from({ length: pages }, (_, n) => n).map((n) => <Button key={n} size="small" appearance={page === n ? 'primary' : 'subtle'} onClick={() => setPage(n)}>轮盘 {n + 1}</Button>)}<small>{chosen.length} / 24</small></div><Dropdown className="slotCount" aria-label="每组软件数量" value={`每组 ${count} 个`} selectedOptions={[String(count)]} onOptionSelect={(_, d) => { if (d.optionValue) void update({ slotsPerPage: Number(d.optionValue), wheel: s.wheel.filter(Boolean) }); }}>{Array.from({ length: 9 }, (_, i) => i + 4).map((n) => <Option key={n} value={String(n)} text={`每组 ${n} 个`}>{`每组 ${n} 个`}</Option>)}</Dropdown><Button appearance="subtle" icon={<Add20Regular />} onClick={() => void api.browse().then(add)}>添加入口</Button></div>
            <div className="catalogToolbar"><TabList selectedValue={category} onTabSelect={(_, d) => { setCategory(String(d.value)); setFilter(''); }}><Tab value="apps" icon={<Apps24Regular />}>软件</Tab><Tab value="outer">外环内容</Tab></TabList>{category !== 'outer' && <Input type="search" value={filter} onChange={(_, d) => setFilter(d.value)} placeholder="搜索软件，支持中文名称" />}</div>
            <WheelEditor onRequestIcon={requestIcon} onInspect={setInspected} inspector={category === 'outer' ? <OuterEditor inspected={inspected} settings={s} nameOf={nameOf} update={update} /> : undefined} wheel={pageWheel} center="" centerArt={<MascotView />} centerOn={false} trigger={trigger} pool={pool} visibleIds={new Set(catalog.map((item) => item.path))} defaults={{ wheel: Array<string>(count).fill(''), center: '' }}
              labels={{ title: '按你的习惯排布', desc: '拖入软件，或点选后放到一个方向。', poolTitle: `已安装的软件 · ${catalog.length}`, poolDesc: '拖到左侧轮盘，或点击选中后再点一个槽位。' }}
              poolExtra={catalog.length === 0 ? <span className="desc">没有匹配的动作；可以换个关键词，或添加文件。</span> : undefined}
              onDropFiles={(files) => {
                const paths = api.filePaths(files);
                if (category !== 'outer') { void add(paths); return; }
                const owner = chosen.includes(inspected) ? inspected : chosen[0];
                if (!owner) return;
                const previous = s.content[owner] ?? [];
                if (previous.length + paths.length > 24) { setError('每个软件最多固定 24 个外环项目'); return; }
                void update({ content: { ...s.content, [owner]: [...previous, ...paths.map((target) => ({ id: crypto.randomUUID(), name: target.split('/').pop() || target, target }))] } });
              }} onPreview={() => undefined}
              onChange={(patch) => { if (!patch.wheel) return; const wheel = applyPagePatch(s.wheel, page, count, patch.wheel); if (wheel.filter(Boolean).length > 24) { setError('轮盘最多可以固定 24 项'); return; } void update({ wheel }); }} />
            {inspected && chosen.includes(inspected) && <div className="selectedItem"><img src={iconOf(inspected)} alt="" /><div><span className="desc">当前入口</span><Input key={inspected} aria-label="入口名称" defaultValue={nameOf(inspected)} onBlur={(e) => { if (e.currentTarget.value !== nameOf(inspected)) void update({ names: { ...s.names, [inspected]: e.currentTarget.value } }); }} /></div><Button appearance="subtle" onClick={() => setCategory('outer')}>编辑外环</Button><Button appearance="subtle" aria-label="移除当前入口" icon={<Dismiss20Regular />} onClick={() => { void update({ wheel: s.wheel.map((path) => path === inspected ? '' : path) }); setInspected(''); }} /></div>}
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
            <SwitchRow icon={<DataPie24Regular />} title="窗口预览" description="悬停软件或内容时显示所在窗口的当前画面，需要屏幕录制授权。" checked={s.windowPreviews} onChange={(on) => { void update({ windowPreviews: on }); if (on && !state.screenPermission) void api.requestWindowPreview(); }} />
            {s.windowPreviews && !state.screenPermission && <SettingRow icon={<Settings24Regular />} title="屏幕录制尚未开启" description="窗口列表和切换仍可使用，开启后才显示预览。"><Button onClick={() => void api.requestWindowPreview()}>授予屏幕录制权限</Button></SettingRow>}
            <div className="section">关于</div><div className="card notes"><img className="aboutLogo" src={logo} alt="" /><div className="lbl"><b>ViGo</b><span className="desc">按住 V，把常用动作放到手边。</span><span className="desc">为桌面打造的 V 键动作轮盘。</span></div></div>
          </>}
        </>}
      </main>
    </div>
  </FluentProvider>;
}

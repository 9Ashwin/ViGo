import { Button, Dropdown, Input, Option } from '@fluentui/react-components';
import { useEffect, useState } from 'react';
import type { Settings } from '../../../shared/settings';
import type { BrowserTab, PinnedContent } from '../../../shared/content';
import { validContentTarget } from '../../../shared/content';
import { api } from '../api';

export function OuterEditor({ settings, nameOf, update, inspected }: { inspected?: string; settings: Settings; nameOf: (path: string) => string; update: (patch: Partial<Settings>) => Promise<void> }) {
  const paths = settings.wheel.filter(Boolean);
  const [selected, setSelected] = useState(paths[0] ?? '');
  useEffect(() => { if (inspected) setSelected(inspected); }, [inspected]);
  const owner = paths.includes(selected) ? selected : paths[0] ?? '';
  const [name, setName] = useState('');
  const [url, setUrl] = useState('');
  const [error, setError] = useState('');
  const [tabs, setTabs] = useState<BrowserTab[]>([]);
  const entries = settings.content[owner] ?? [];
  useEffect(() => {
    let cancelled = false;
    setTabs([]);
    const refresh = () => { void api.tabs(owner).then((next) => { if (!cancelled) setTabs(next); }).catch(() => undefined); };
    refresh(); window.addEventListener('focus', refresh);
    return () => { cancelled = true; window.removeEventListener('focus', refresh); };
  }, [owner]);
  const save = (next: PinnedContent[]) => update({ content: { ...settings.content, [owner]: next } });
  async function add(targets: string[]) {
    if (entries.length + targets.length > 24) { setError('每个软件最多固定 24 个外环项目'); return; }
    await save([...entries, ...targets.map((target) => ({ id: crypto.randomUUID(), target, name: target.split('/').filter(Boolean).pop() ?? target }))]);
    setError('');
  }
  if (!owner) return null;
  return <section className="outerEditor" aria-label="外环内容">
    <header><div><h3>外环内容</h3><span className="desc">文件夹、常用网址与当前标签，跟随软件展开。</span></div>
      <Dropdown aria-label="选择外环所属软件" value={nameOf(owner)} selectedOptions={[owner]} onOptionSelect={(_, data) => setSelected(data.optionValue ?? owner)}>
        {paths.map((path) => <Option key={path} value={path}>{nameOf(path)}</Option>)}
      </Dropdown></header>
    <div className="outerActions"><Button onClick={() => void api.browse().then(add)}>添加文件或文件夹</Button><span className="desc">固定 {entries.length} / 24 · 全部展开</span></div>
    <form className="outerUrl" onSubmit={(event) => { event.preventDefault(); if (!/^https?:\/\//.test(url) || !validContentTarget(url)) { setError('请输入 http 或 https 网址'); return; } if (entries.length >= 24) { setError('每个软件最多固定 24 个外环项目'); return; } void save([...entries, { id: crypto.randomUUID(), name: name.trim() || new URL(url).hostname, target: url }]).then(() => { setUrl(''); setName(''); setError(''); }); }}>
      <Input aria-label="网址名称" placeholder="名称" value={name} onChange={(_, d) => setName(d.value)} />
      <Input aria-label="网址" placeholder="https://…" value={url} onChange={(_, d) => setUrl(d.value)} /><Button type="submit">添加网址</Button>
    </form>
    {error && <div role="alert" className="inlineError">{error}</div>}
    <div className="outerRows">{entries.map((entry, index) => <div key={entry.id} className="outerRow">
      <Input aria-label="外环项目名称" defaultValue={entry.name} onBlur={(event) => { const value = event.currentTarget.value.trim(); if (value && value !== entry.name) void save(entries.map((item) => item.id === entry.id ? { ...item, name: value } : item)); }} />
      <span className="outerTarget" title={entry.target}>{entry.target}</span>
      <Button size="small" aria-label="上移外环项目" disabled={!index} onClick={() => { const next = [...entries]; [next[index - 1], next[index]] = [next[index], next[index - 1]]; void save(next); }}>↑</Button>
      <Button size="small" aria-label="下移外环项目" disabled={index === entries.length - 1} onClick={() => { const next = [...entries]; [next[index + 1], next[index]] = [next[index], next[index + 1]]; void save(next); }}>↓</Button>
      <Button size="small" onClick={() => void save(entries.filter((item) => item.id !== entry.id))}>移除</Button>
    </div>)}</div>
    {tabs.length > 0 && <div className="liveTabs"><b>当前浏览器标签 · {tabs.length}</b><span className="desc">自动加入外环，松手切换原标签；固定网址会使用所属浏览器打开。</span>{tabs.map((tab) => <div key={tab.id}>{tab.selected ? '● ' : '○ '}{tab.title}</div>)}</div>}
  </section>;
}

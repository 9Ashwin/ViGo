export const useText = () => ({
  lang: 'zh', on: '开启', off: '关闭', wheelReset: '清空当前页', wheelCenter: 'V',
  slotNames: ['上', '右上', '右', '右下', '下', '左下', '左', '左上'],
  wheelSlotLabel: (position: string, name: string) => `${position} · ${name}`,
  wheelCenterLabel: (trigger: string, _name: string) => `按住 ${trigger} 呼出`,
  wheelPickHint: (name: string) => `已选中 ${name}，点击目标槽位；Esc 取消`,
  removeImage: (name: string) => `移除 ${name}`,
  pressKey: '按一个键…', hintUnusable: '这个按键暂不支持', hintNotTrigger: '请选择不会影响输入的按键', hintNeedModifier: '请包含修饰键',
  chooseKey: '选择按键', customKey: '自定义按键', customKeyMenu: '自定义按键…',
  emoteKeyOff: '关闭', keyboard: '键盘', mouse: '鼠标', mouse4: '鼠标侧键 X1', mouse5: '鼠标侧键 X2', mouseBack: '后退', mouseForward: '前进',
});

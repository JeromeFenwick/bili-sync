<script lang="ts">
	import { onMount, tick } from 'svelte';
	import { Button } from '$lib/components/ui/button/index.js';
	import { Input } from '$lib/components/ui/input/index.js';
	import { Label } from '$lib/components/ui/label/index.js';
	import { Switch } from '$lib/components/ui/switch/index.js';
	import * as Tabs from '$lib/components/ui/tabs/index.js';
	import { Separator } from '$lib/components/ui/separator/index.js';
	import { Badge } from '$lib/components/ui/badge/index.js';
	import * as Dialog from '$lib/components/ui/dialog/index.js';
	import * as Tooltip from '$lib/components/ui/tooltip/index.js';
	import PasswordInput from '$lib/components/custom/password-input.svelte';
	import QrLogin from '$lib/components/custom/qr-login.svelte';
	import NotifierDialog from './NotifierDialog.svelte';
	import InfoIcon from '@lucide/svelte/icons/info';
	import QrCodeIcon from '@lucide/svelte/icons/qr-code';
	import api from '$lib/api';
	import { toast } from 'svelte-sonner';
	import { setBreadcrumb } from '$lib/stores/breadcrumb';
	import type { Config, ApiError, Notifier, Credential } from '$lib/types';

	let frontendToken = $state(''); // 前端认证token
	let config = $state<Config | null>(null);
	let formData = $state<Config | null>(null);
	let saving = $state(false);
	let loading = $state(false);

	let intervalInput = $state<string>('1200');
	let dailySummaryHour = $state(9);
	let dailySummaryMinute = $state(0);
	let dailySummaryHourInput = $state('09');
	let dailySummaryMinuteInput = $state('00');
	
	// 静默时间段相关
	let quietHoursStartInput = $state('22');
	let quietHoursEndInput = $state('09');

	// Notifier 管理相关
	let showNotifierDialog = $state(false);
	let editingNotifier = $state<Notifier | null>(null);
	let editingNotifierIndex = $state<number | null>(null);
	let isEditing = $state(false);

	// QR 登录 Dialog 相关
	let showQrLoginDialog = $state(false);
	let qrLoginComponent: QrLogin;

	function openAddNotifierDialog() {
		editingNotifier = null;
		editingNotifierIndex = null;
		isEditing = false;
		showNotifierDialog = true;
	}

	function openEditNotifierDialog(notifier: Notifier, index: number) {
		editingNotifier = { ...notifier };
		editingNotifierIndex = index;
		isEditing = true;
		showNotifierDialog = true;
	}

	function closeNotifierDialog() {
		showNotifierDialog = false;
		editingNotifier = null;
		editingNotifierIndex = null;
		isEditing = false;
	}

	function addNotifier(notifier: Notifier) {
		if (!formData) return;
		if (!formData.notifiers) {
			formData.notifiers = [];
		}
		formData.notifiers = [...formData.notifiers, notifier];
		closeNotifierDialog();
	}

	function updateNotifier(index: number, notifier: Notifier) {
		if (!formData?.notifiers) return;
		const newNotifiers = [...formData.notifiers];
		newNotifiers[index] = notifier;
		formData.notifiers = newNotifiers;
		closeNotifierDialog();
	}

	function deleteNotifier(index: number) {
		if (!formData?.notifiers) return;
		formData.notifiers = formData.notifiers.filter((_, i) => i !== index);
	}

	async function testNotifier(notifier: Notifier) {
		try {
			const response = await api.testNotifier(notifier);
			const result = response.data;
			if (result.success) {
				toast.success(result.message, {
					description: result.details || undefined
				});
			} else {
				toast.error(result.message, {
					description: result.details || undefined
				});
			}
		} catch (error) {
			console.error('测试通知失败:', error);
			toast.error('测试通知失败', {
				description: (error as ApiError).message
			});
		}
	}

	async function loadConfig() {
		loading = true;
		try {
			const response = await api.getConfig();
			config = response.data;
			formData = { ...config };
			
			// 设置默认值（如果后端没有返回这些字段）
			if (formData.notify_new_videos === undefined) {
				formData.notify_new_videos = false;
			}
			if (formData.notify_daily_summary === undefined) {
				formData.notify_daily_summary = false;
			}
			if (formData.notification_interval === undefined) {
				formData.notification_interval = 5;
			}
			if (formData.daily_summary_cron === undefined) {
				formData.daily_summary_cron = '0 0 9 * * *';
			}

			// 订阅时是否自动启用视频源（后端新增字段，老配置需要兜底）
			if (formData.enable_video_source_on_subscribe === undefined) {
				formData.enable_video_source_on_subscribe = true;
			}
			
			// 解析每日汇总时间的 cron 表达式
			// cron 格式：秒 分 时 日 月 周，例如 "0 0 9 * * *" 表示每天9点0分0秒
			if (formData.daily_summary_cron) {
				const cronMatch = formData.daily_summary_cron.match(/^0\s+(\d+)\s+(\d+)\s+\*\s+\*\s+\*$/);
				if (cronMatch) {
					const minute = parseInt(cronMatch[1], 10);
					const hour = parseInt(cronMatch[2], 10);
					// 只有当解析结果为 NaN 时才使用默认值，0 是有效值
					dailySummaryMinute = isNaN(minute) ? 0 : minute;
					dailySummaryHour = isNaN(hour) ? 9 : hour;
					// 同步更新输入框显示值
					dailySummaryHourInput = String(dailySummaryHour).padStart(2, '0');
					dailySummaryMinuteInput = String(dailySummaryMinute).padStart(2, '0');
				} else {
					// 如果格式不匹配，使用默认值并更新 cron 表达式
					dailySummaryHour = 9;
					dailySummaryMinute = 0;
					dailySummaryHourInput = '09';
					dailySummaryMinuteInput = '00';
					formData.daily_summary_cron = `0 0 9 * * *`;
				}
			} else {
				// 如果没有 cron 表达式，使用默认值
				dailySummaryHour = 9;
				dailySummaryMinute = 0;
				dailySummaryHourInput = '09';
				dailySummaryMinuteInput = '00';
				formData.daily_summary_cron = `0 0 9 * * *`;
			}

			// 根据 interval 的类型初始化输入框
			if (typeof formData.interval === 'number') {
				intervalInput = String(formData.interval);
			} else {
				intervalInput = formData.interval;
			}
			
			// 加载静默时间段配置
			if (formData.enable_notification_quiet_hours === undefined) {
				formData.enable_notification_quiet_hours = false;
			}
			if (formData.quiet_hours_start === undefined) {
				formData.quiet_hours_start = 22;
			}
			if (formData.quiet_hours_end === undefined) {
				formData.quiet_hours_end = 9;
			}
			quietHoursStartInput = String(formData.quiet_hours_start).padStart(2, '0');
			quietHoursEndInput = String(formData.quiet_hours_end).padStart(2, '0');
		} catch (error) {
			console.error('加载配置失败:', error);
			const apiError = error as ApiError;
			
			// 如果是认证错误（401），清除认证信息
			if (apiError.status === 401) {
				formData = null;
				config = null;
				frontendToken = '';
				api.clearAuthToken();
				toast.error('认证已失效', {
					description: '请重新输入 Token 进行认证'
				});
			} else {
				toast.error('加载配置失败', {
					description: apiError.message
				});
			}
		} finally {
			loading = false;
		}
	}

	async function authenticateFrontend() {
		if (!frontendToken.trim()) {
			toast.error('请输入前端认证Token');
			return;
		}

		try {
			api.setAuthToken(frontendToken.trim());
			localStorage.setItem('authToken', frontendToken.trim());
			await loadConfig();
			toast.success('前端认证成功');
		} catch (error) {
			console.error('前端认证失败:', error);
			toast.error('认证失败，请检查Token是否正确');
		}
	}

	async function saveConfig() {
		if (!formData) {
			toast.error('配置未加载');
			return;
		}

		// 保存前根据输入内容判断类型
		const trimmed = intervalInput.trim();
		const asNumber = Number(trimmed);

		if (!isNaN(asNumber) && trimmed !== '') {
			// 纯数字，作为 Interval
			formData.interval = asNumber;
		} else {
			// 非数字，作为 Cron 表达式
			formData.interval = trimmed;
		}

		// 确保每日汇总的 cron 表达式根据当前输入的时间更新
		if (formData.notify_daily_summary) {
			// 从输入框状态读取最新值
			let hour = dailySummaryHour;
			let minute = dailySummaryMinute;
			
			if (dailySummaryHourInput !== '') {
				const hourValue = parseInt(dailySummaryHourInput, 10);
				if (!isNaN(hourValue)) {
					hour = Math.max(0, Math.min(23, hourValue));
					dailySummaryHour = hour;
					dailySummaryHourInput = String(hour).padStart(2, '0');
				}
			}
			
			if (dailySummaryMinuteInput !== '') {
				const minuteValue = parseInt(dailySummaryMinuteInput, 10);
				if (!isNaN(minuteValue)) {
					minute = Math.max(0, Math.min(59, minuteValue));
					dailySummaryMinute = minute;
					dailySummaryMinuteInput = String(minute).padStart(2, '0');
				}
			}
			
			// 确保小时和分钟在有效范围内，并更新 cron 表达式
			hour = Math.max(0, Math.min(23, hour));
			minute = Math.max(0, Math.min(59, minute));
			formData.daily_summary_cron = `0 ${minute} ${hour} * * *`;
			
			// 同步更新 state 和输入框显示
			dailySummaryHour = hour;
			dailySummaryMinute = minute;
			dailySummaryHourInput = String(hour).padStart(2, '0');
			dailySummaryMinuteInput = String(minute).padStart(2, '0');
		}
		
		// 保存静默时间段配置
		if (formData.enable_notification_quiet_hours) {
			// 从输入框读取并验证静默时间段
			let startHour = formData.quiet_hours_start;
			let endHour = formData.quiet_hours_end;
			
			if (quietHoursStartInput !== '') {
				const startValue = parseInt(quietHoursStartInput, 10);
				if (!isNaN(startValue)) {
					startHour = Math.max(0, Math.min(23, startValue));
					formData.quiet_hours_start = startHour;
					quietHoursStartInput = String(startHour).padStart(2, '0');
				}
			}
			
			if (quietHoursEndInput !== '') {
				const endValue = parseInt(quietHoursEndInput, 10);
				if (!isNaN(endValue)) {
					endHour = Math.max(0, Math.min(23, endValue));
					formData.quiet_hours_end = endHour;
					quietHoursEndInput = String(endHour).padStart(2, '0');
				}
			}
		}

		saving = true;
		try {
			let resp = await api.updateConfig(formData);
			formData = resp.data;
			config = { ...formData };

			// 更新输入框显示
			if (typeof formData.interval === 'number') {
				intervalInput = String(formData.interval);
			} else {
				intervalInput = formData.interval;
			}

			// 重新解析每日汇总时间的 cron 表达式，确保界面显示正确
			if (formData.daily_summary_cron) {
				const cronMatch = formData.daily_summary_cron.match(/^0\s+(\d+)\s+(\d+)\s+\*\s+\*\s+\*$/);
				if (cronMatch) {
					const minute = parseInt(cronMatch[1], 10);
					const hour = parseInt(cronMatch[2], 10);
					if (!isNaN(minute) && !isNaN(hour)) {
						dailySummaryMinute = minute;
						dailySummaryHour = hour;
						// 同步更新输入框显示值
						dailySummaryHourInput = String(hour).padStart(2, '0');
						dailySummaryMinuteInput = String(minute).padStart(2, '0');
					}
				}
				// 如果解析失败，保持当前值不变，不设置默认值
			}

			toast.success('配置已保存');
		} catch (error) {
			console.error('保存配置失败:', error);
			toast.error('保存配置失败', {
				description: (error as ApiError).message
			});
		} finally {
			saving = false;
		}
	}

	function handleQrLoginSuccess(credential: Credential) {
		if (!formData) return;

		// 自动填充凭证到 formData
		formData.credential = credential;

		toast.success('扫码登录成功，已填充凭据');

		// 自动保存配置
		saveConfig();

		// 关闭弹窗
		showQrLoginDialog = false;
	}

	onMount(() => {
		setBreadcrumb([{ label: '设置' }]);

		const savedToken = localStorage.getItem('authToken');
		if (savedToken) {
			frontendToken = savedToken;
			api.setAuthToken(savedToken);
		}

		loadConfig();
	});
</script>

<svelte:head>
	<title>设置 - Bili Sync</title>
</svelte:head>

<div class="space-y-6">
	<!-- 前端认证状态栏 -->
	<div class="bg-card rounded-lg border p-4">
		<div class="flex items-center justify-between">
			<div class="space-y-1">
				<h2 class="font-semibold">前端认证状态</h2>
				<p class="text-muted-foreground text-sm">
					{formData ? '已认证 - 可以正常加载数据' : '未认证 - 请输入 Token 进行鉴权'}
				</p>
			</div>
			{#if !formData}
				<div class="flex gap-3">
					<PasswordInput bind:value={frontendToken} placeholder="输入认证Token" />
					<Button onclick={authenticateFrontend} disabled={!frontendToken.trim()}>认证</Button>
				</div>
			{:else}
				<div class="flex items-center gap-3">
					<div class="flex items-center gap-2">
						<div class="h-2 w-2 rounded-full bg-emerald-500"></div>
						<span class="text-sm text-emerald-600">已认证</span>
					</div>
					<Button
						variant="outline"
						size="sm"
						onclick={() => {
							formData = null;
							config = null;
							localStorage.removeItem('authToken');
							api.clearAuthToken();
							frontendToken = '';
						}}
					>
						退出认证
					</Button>
				</div>
			{/if}
		</div>
	</div>

	<!-- 应用配置 -->
	{#if loading}
		<div class="flex items-center justify-center py-16">
			<div class="space-y-2 text-center">
				<div
					class="border-primary mx-auto h-6 w-6 animate-spin rounded-full border-2 border-t-transparent"
				></div>
				<p class="text-muted-foreground">加载配置中...</p>
			</div>
		</div>
	{:else if formData}
		<div class="space-y-6">
			<Tabs.Root value="basic" class="w-full">
				<Tabs.List class="grid w-full grid-cols-6">
					<Tabs.Trigger value="basic">基本设置</Tabs.Trigger>
					<Tabs.Trigger value="auth">B站认证</Tabs.Trigger>
					<Tabs.Trigger value="filter">视频处理</Tabs.Trigger>
					<Tabs.Trigger value="danmaku">弹幕渲染</Tabs.Trigger>
					<Tabs.Trigger value="notifiers">通知设置</Tabs.Trigger>
					<Tabs.Trigger value="advanced">高级设置</Tabs.Trigger>
				</Tabs.List>

				<!-- 基本设置 -->
				<Tabs.Content value="basic" class="mt-6 space-y-6">
					<div class="grid grid-cols-1 gap-6 lg:grid-cols-2">
						<div class="space-y-2">
							<Label for="bind-address">绑定地址</Label>
							<Input
								id="bind-address"
								placeholder="127.0.0.1:9999"
								bind:value={formData.bind_address}
							/>
						</div>
						<div class="space-y-2">
							<div class="flex items-center gap-1">
								<Label for="interval">任务触发条件</Label>
								<Tooltip.Root>
									<Tooltip.Trigger>
										<InfoIcon class="text-muted-foreground h-3.5 w-3.5" />
									</Tooltip.Trigger>
									<Tooltip.Content>
										<p class="text-xs">
											视频下载任务的触发条件，支持两种格式：<br />
											1. 输入数字表示间隔秒数，例如 1200 表示每隔 20 分钟触发一次； <br />
											2. 输入 Cron 表达式，格式为“秒 分 时 日 月 周”，例如“0 0 2 * * *”表示每天凌晨2点触发一次。
										</p>
									</Tooltip.Content>
								</Tooltip.Root>
							</div>
							<Input
								id="interval"
								type="text"
								bind:value={intervalInput}
								placeholder="1200 或 0 0 2 * * *"
							/>
						</div>
						<div class="space-y-2">
							<Label for="video-name">视频名称模板</Label>
							<Input id="video-name" bind:value={formData.video_name} />
						</div>
						<div class="space-y-2">
							<Label for="page-name">分页名称模板</Label>
							<Input id="page-name" bind:value={formData.page_name} />
						</div>
						<div class="space-y-2">
							<Label for="upper-path">UP主头像保存路径</Label>
							<Input
								id="upper-path"
								placeholder="/path/to/upper/faces"
								bind:value={formData.upper_path}
							/>
						</div>
						<div class="space-y-2">
							<Label for="time-format">时间格式</Label>
							<Input
								id="time-format"
								placeholder="%Y-%m-%d %H:%M:%S"
								bind:value={formData.time_format}
							/>
						</div>
					</div>

					<div class="space-y-4">
						<div class="space-y-2">
							<Label for="backend-auth-token">后端 API 认证Token</Label>
							<PasswordInput
								id="backend-auth-token"
								placeholder="设置后端 API 认证Token"
								bind:value={formData.auth_token}
							/>
							<p class="text-muted-foreground text-xs">
								修改此Token后，前端需要使用新Token重新认证才能访问API
							</p>
						</div>
					</div>

					<Separator />

					<div class="grid grid-cols-1 gap-6 lg:grid-cols-2">
						<div class="space-y-2">
							<Label for="favorite-default-path">收藏夹快捷订阅路径模板</Label>
							<Input id="favorite-default-path" bind:value={formData.favorite_default_path} />
						</div>
						<div class="space-y-2">
							<Label for="collection-default-path">合集快捷订阅路径模板</Label>
							<Input id="collection-default-path" bind:value={formData.collection_default_path} />
						</div>
						<div class="space-y-2">
							<Label for="submission-default-path">UP 主投稿快捷订阅路径模板</Label>
							<Input id="submission-default-path" bind:value={formData.submission_default_path} />
						</div>
					</div>

					<Separator />

					<div class="space-y-4">
						<div class="flex items-center space-x-2">
							<Switch id="cdn-sorting" bind:checked={formData.cdn_sorting} />
							<Label for="cdn-sorting">启用CDN排序</Label>
						</div>
						<div class="flex items-center space-x-2">
							<Switch id="enable-cover-background" bind:checked={formData.enable_cover_background} />
							<Label for="enable-cover-background">开启封面背景渲染</Label>
						</div>
						<div class="flex items-center space-x-2">
							<Switch
								id="enable-video-source-on-subscribe"
								bind:checked={formData.enable_video_source_on_subscribe}
							/>
							<Label for="enable-video-source-on-subscribe">
								订阅收藏夹 / 合集 / UP 投稿时自动启用对应视频源
							</Label>
						</div>
					</div>
				</Tabs.Content>

				<!-- B站认证 -->
				<Tabs.Content value="auth" class="mt-6 space-y-6">
					<div class="flex items-center justify-between">
						<div class="space-y-1">
							<Label class="text-base font-semibold">快速登录</Label>
							<p class="text-muted-foreground text-sm">使用哔哩哔哩 APP 扫码登录，自动填充凭据</p>
						</div>
						<Button
							onclick={() => {
								showQrLoginDialog = true;
								tick().then(() => {
									qrLoginComponent!.init();
								});
							}}
						>
							<QrCodeIcon class="mr-2 h-4 w-4" />
							扫码登录
						</Button>
					</div>

					<Separator />

					<!-- 原有的手动输入 Cookie 表单 -->
					<div class="space-y-4">
						<div class="space-y-2">
							<Label for="sessdata">SESSDATA</Label>
							<PasswordInput
								id="sessdata"
								placeholder="请输入SESSDATA"
								bind:value={formData.credential.sessdata}
							/>
						</div>
						<div class="space-y-2">
							<Label for="bili-jct">bili_jct</Label>
							<PasswordInput
								id="bili-jct"
								placeholder="请输入bili_jct"
								bind:value={formData.credential.bili_jct}
							/>
						</div>
						<div class="space-y-2">
							<Label for="buvid3">buvid3</Label>
							<PasswordInput
								id="buvid3"
								placeholder="请输入buvid3"
								bind:value={formData.credential.buvid3}
							/>
						</div>
						<div class="space-y-2">
							<Label for="dedeuserid">dedeuserid</Label>
							<PasswordInput
								id="dedeuserid"
								placeholder="请输入dedeuserid"
								bind:value={formData.credential.dedeuserid}
							/>
						</div>
						<div class="space-y-2">
							<Label for="ac-time-value">ac_time_value</Label>
							<PasswordInput
								id="ac-time-value"
								placeholder="请输入ac_time_value"
								bind:value={formData.credential.ac_time_value}
							/>
						</div>
					</div>
				</Tabs.Content>

				<!-- 过滤规则 -->
				<Tabs.Content value="filter" class="mt-6 space-y-6">
					<div class="grid grid-cols-1 gap-6 lg:grid-cols-2">
						<div class="space-y-2">
							<Label for="video-max-quality">最高视频质量</Label>
							<select
								id="video-max-quality"
								class="border-input bg-background ring-offset-background placeholder:text-muted-foreground focus-visible:ring-ring flex h-10 w-full rounded-md border px-3 py-2 text-sm file:border-0 file:bg-transparent file:text-sm file:font-medium focus-visible:ring-2 focus-visible:ring-offset-2 focus-visible:outline-none disabled:cursor-not-allowed disabled:opacity-50"
								bind:value={formData.filter_option.video_max_quality}
							>
								<option value="Quality360p">360p</option>
								<option value="Quality480p">480p</option>
								<option value="Quality720p">720p</option>
								<option value="Quality1080p">1080p</option>
								<option value="Quality1080pPLUS">1080p+</option>
								<option value="Quality1080p60">1080p60</option>
								<option value="Quality4k">4K</option>
								<option value="QualityHdr">HDR</option>
								<option value="QualityDolby">杜比视界</option>
								<option value="Quality8k">8K</option>
							</select>
						</div>
						<div class="space-y-2">
							<Label for="video-min-quality">最低视频质量</Label>
							<select
								id="video-min-quality"
								class="border-input bg-background ring-offset-background placeholder:text-muted-foreground focus-visible:ring-ring flex h-10 w-full rounded-md border px-3 py-2 text-sm file:border-0 file:bg-transparent file:text-sm file:font-medium focus-visible:ring-2 focus-visible:ring-offset-2 focus-visible:outline-none disabled:cursor-not-allowed disabled:opacity-50"
								bind:value={formData.filter_option.video_min_quality}
							>
								<option value="Quality360p">360p</option>
								<option value="Quality480p">480p</option>
								<option value="Quality720p">720p</option>
								<option value="Quality1080p">1080p</option>
								<option value="Quality1080pPLUS">1080p+</option>
								<option value="Quality1080p60">1080p60</option>
								<option value="Quality4k">4K</option>
								<option value="QualityHdr">HDR</option>
								<option value="QualityDolby">杜比视界</option>
								<option value="Quality8k">8K</option>
							</select>
						</div>
						<div class="space-y-2">
							<Label for="audio-max-quality">最高音频质量</Label>
							<select
								id="audio-max-quality"
								class="border-input bg-background ring-offset-background placeholder:text-muted-foreground focus-visible:ring-ring flex h-10 w-full rounded-md border px-3 py-2 text-sm file:border-0 file:bg-transparent file:text-sm file:font-medium focus-visible:ring-2 focus-visible:ring-offset-2 focus-visible:outline-none disabled:cursor-not-allowed disabled:opacity-50"
								bind:value={formData.filter_option.audio_max_quality}
							>
								<option value="Quality64k">64k</option>
								<option value="Quality132k">132k</option>
								<option value="Quality192k">192k</option>
								<option value="QualityDolby">杜比全景声</option>
								<option value="QualityHiRES">Hi-RES</option>
							</select>
						</div>
						<div class="space-y-2">
							<Label for="audio-min-quality">最低音频质量</Label>
							<select
								id="audio-min-quality"
								class="border-input bg-background ring-offset-background placeholder:text-muted-foreground focus-visible:ring-ring flex h-10 w-full rounded-md border px-3 py-2 text-sm file:border-0 file:bg-transparent file:text-sm file:font-medium focus-visible:ring-2 focus-visible:ring-offset-2 focus-visible:outline-none disabled:cursor-not-allowed disabled:opacity-50"
								bind:value={formData.filter_option.audio_min_quality}
							>
								<option value="Quality64k">64k</option>
								<option value="Quality132k">132k</option>
								<option value="Quality192k">192k</option>
								<option value="QualityDolby">杜比全景声</option>
								<option value="QualityHiRES">Hi-RES</option>
							</select>
						</div>
					</div>

					<Separator />

					<div class="space-y-4">
						<Label>视频编码格式偏好（按优先级排序）</Label>
						<p class="text-muted-foreground text-sm">排在前面的编码格式优先级更高</p>
						<div class="space-y-2">
							{#each formData.filter_option.codecs as codec, index (index)}
								<div class="flex items-center space-x-2 rounded-lg border p-3">
									<Badge variant="secondary">{index + 1}</Badge>
									<span class="flex-1 font-medium">{codec}</span>
									<div class="flex space-x-1">
										<Button
											type="button"
											size="sm"
											variant="outline"
											disabled={index === 0}
											onclick={() => {
												if (formData && index > 0) {
													const newCodecs = [...formData.filter_option.codecs];
													[newCodecs[index - 1], newCodecs[index]] = [
														newCodecs[index],
														newCodecs[index - 1]
													];
													formData.filter_option.codecs = newCodecs;
												}
											}}
										>
											↑
										</Button>
										<Button
											type="button"
											size="sm"
											variant="outline"
											disabled={index === formData.filter_option.codecs.length - 1}
											onclick={() => {
												if (formData && index < formData.filter_option.codecs.length - 1) {
													const newCodecs = [...formData.filter_option.codecs];
													[newCodecs[index], newCodecs[index + 1]] = [
														newCodecs[index + 1],
														newCodecs[index]
													];
													formData.filter_option.codecs = newCodecs;
												}
											}}
										>
											↓
										</Button>
										<Button
											type="button"
											size="sm"
											variant="destructive"
											onclick={() => {
												if (formData) {
													formData.filter_option.codecs = formData.filter_option.codecs.filter(
														(_, i) => i !== index
													);
												}
											}}
										>
											×
										</Button>
									</div>
								</div>
							{/each}

							{#if formData.filter_option.codecs.length < 3}
								<div class="space-y-2">
									<Label>添加编码格式</Label>
									<div class="flex gap-2">
										{#each ['AV1', 'HEV', 'AVC'] as codec (codec)}
											{#if !formData.filter_option.codecs.includes(codec)}
												<Button
													type="button"
													size="sm"
													variant="outline"
													onclick={() => {
														if (formData) {
															formData.filter_option.codecs = [
																...formData.filter_option.codecs,
																codec
															];
														}
													}}
												>
													+ {codec}
												</Button>
											{/if}
										{/each}
									</div>
								</div>
							{/if}
						</div>
					</div>

					<Separator />

					<div class="space-y-4">
						<Label>特殊流排除选项</Label>
						<p class="text-muted-foreground text-sm">排除某些类型的特殊流</p>
						<div class="flex items-center space-x-2">
							<Switch id="no-dolby-video" bind:checked={formData.filter_option.no_dolby_video} />
							<Label for="no-dolby-video">排除杜比视界视频</Label>
						</div>
						<div class="flex items-center space-x-2">
							<Switch id="no-dolby-audio" bind:checked={formData.filter_option.no_dolby_audio} />
							<Label for="no-dolby-audio">排除杜比全景声音频</Label>
						</div>
						<div class="flex items-center space-x-2">
							<Switch id="no-hdr" bind:checked={formData.filter_option.no_hdr} />
							<Label for="no-hdr">排除HDR视频</Label>
						</div>
						<div class="flex items-center space-x-2">
							<Switch id="no-hires" bind:checked={formData.filter_option.no_hires} />
							<Label for="no-hires">排除Hi-RES音频</Label>
						</div>
					</div>

					<Separator />

					<div class="space-y-4">
						<Label>处理跳过选项</Label>
						<p class="text-muted-foreground text-sm">在视频处理部分跳过某些执行环节</p>
						<div class="flex items-center space-x-2">
							<Switch id="skip-poster" bind:checked={formData.skip_option.no_poster} />
							<Label for="skip-poster">跳过视频封面</Label>
						</div>
						<div class="flex items-center space-x-2">
							<Switch id="skip-video-nfo" bind:checked={formData.skip_option.no_video_nfo} />
							<Label for="skip-video-nfo">跳过视频 NFO</Label>
						</div>
						<div class="flex items-center space-x-2">
							<Switch id="skip-upper-info" bind:checked={formData.skip_option.no_upper} />
							<Label for="skip-upper-info">跳过 Up 主头像、信息</Label>
						</div>
						<div class="flex items-center space-x-2">
							<Switch id="skip-danmaku" bind:checked={formData.skip_option.no_danmaku} />
							<Label for="skip-danmaku">跳过弹幕</Label>
						</div>
						<div class="flex items-center space-x-2">
							<Switch id="skip-subtitle" bind:checked={formData.skip_option.no_subtitle} />
							<Label for="skip-subtitle">跳过字幕</Label>
						</div>
					</div>
				</Tabs.Content>

				<!-- 弹幕设置 -->
				<Tabs.Content value="danmaku" class="mt-6 space-y-6">
					<div class="grid grid-cols-1 gap-6 lg:grid-cols-2">
						<div class="space-y-2">
							<Label for="danmaku-duration">弹幕持续时间（秒）</Label>
							<Input
								id="danmaku-duration"
								type="number"
								min="1"
								step="0.1"
								bind:value={formData.danmaku_option.duration}
							/>
						</div>
						<div class="space-y-2">
							<Label for="danmaku-font">字体</Label>
							<Input
								id="danmaku-font"
								placeholder="黑体"
								bind:value={formData.danmaku_option.font}
							/>
						</div>
						<div class="space-y-2">
							<Label for="danmaku-font-size">字体大小</Label>
							<Input
								id="danmaku-font-size"
								type="number"
								min="8"
								max="72"
								bind:value={formData.danmaku_option.font_size}
							/>
						</div>
						<div class="space-y-2">
							<Label for="danmaku-width-ratio">宽度比例</Label>
							<Input
								id="danmaku-width-ratio"
								type="number"
								min="0.1"
								max="3"
								step="0.1"
								bind:value={formData.danmaku_option.width_ratio}
							/>
						</div>
						<div class="space-y-2">
							<Label for="danmaku-horizontal-gap">水平间距</Label>
							<Input
								id="danmaku-horizontal-gap"
								type="number"
								min="0"
								step="1"
								bind:value={formData.danmaku_option.horizontal_gap}
							/>
						</div>
						<div class="space-y-2">
							<Label for="danmaku-lane-size">轨道大小</Label>
							<Input
								id="danmaku-lane-size"
								type="number"
								min="8"
								max="128"
								bind:value={formData.danmaku_option.lane_size}
							/>
						</div>
						<div class="space-y-2">
							<Label for="danmaku-float-percentage">滚动弹幕高度百分比</Label>
							<Input
								id="danmaku-float-percentage"
								type="number"
								min="0"
								max="1"
								step="0.01"
								bind:value={formData.danmaku_option.float_percentage}
							/>
						</div>
						<div class="space-y-2">
							<Label for="danmaku-bottom-percentage">底部弹幕高度百分比</Label>
							<Input
								id="danmaku-bottom-percentage"
								type="number"
								min="0"
								max="1"
								step="0.01"
								bind:value={formData.danmaku_option.bottom_percentage}
							/>
						</div>
						<div class="space-y-2">
							<Label for="danmaku-opacity">透明度 (0-255)</Label>
							<Input
								id="danmaku-opacity"
								type="number"
								min="0"
								max="255"
								bind:value={formData.danmaku_option.opacity}
							/>
						</div>
						<div class="space-y-2">
							<Label for="danmaku-outline">描边宽度</Label>
							<Input
								id="danmaku-outline"
								type="number"
								min="0"
								max="5"
								step="0.1"
								bind:value={formData.danmaku_option.outline}
							/>
						</div>
						<div class="space-y-2">
							<Label for="danmaku-time-offset">时间偏移（秒）</Label>
							<Input
								id="danmaku-time-offset"
								type="number"
								step="0.1"
								bind:value={formData.danmaku_option.time_offset}
							/>
						</div>
					</div>

					<Separator />

					<div class="space-y-4">
						<div class="flex items-center space-x-2">
							<Switch id="danmaku-bold" bind:checked={formData.danmaku_option.bold} />
							<Label for="danmaku-bold">粗体显示</Label>
						</div>
					</div>
				</Tabs.Content>

				<!-- 通知设置 -->
				<Tabs.Content value="notifiers" class="mt-6 space-y-6">
					<div class="space-y-6">
						<!-- 通知选项 -->
						<div class="space-y-4 rounded-lg border p-4">
							<h3 class="text-lg font-semibold">通知选项</h3>
							<div class="space-y-4">
								<div class="flex items-center justify-between">
									<div class="space-y-0.5">
										<Label for="notify-new-videos">订阅新增通知</Label>
										<p class="text-muted-foreground text-sm">
											当收藏夹/合集/UP投稿有新增视频时发送通知
										</p>
									</div>
									<Switch
										id="notify-new-videos"
										bind:checked={formData.notify_new_videos}
									/>
								</div>
								<Separator />
								<div class="flex items-center justify-between">
									<div class="space-y-0.5">
										<Label for="notify-daily-summary">每日汇总通知</Label>
										<p class="text-muted-foreground text-sm">
											定时发送所有视频的汇总消息
										</p>
									</div>
									<Switch
										id="notify-daily-summary"
										bind:checked={formData.notify_daily_summary}
									/>
								</div>
								{#if formData.notify_daily_summary}
									<div class="space-y-2">
										<Label for="daily-summary-time">发送时间</Label>
										<div class="flex items-center gap-2">
											<select
												id="daily-summary-hour"
												class="border-input bg-background ring-offset-background h-10 rounded-md border px-3 py-2 text-sm focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-2 focus-visible:outline-none"
												bind:value={dailySummaryHourInput}
												on:change={(e) => {
													const value = e.currentTarget.value;
													dailySummaryHourInput = value;
													const num = parseInt(value, 10);
													dailySummaryHour = Math.max(0, Math.min(23, isNaN(num) ? 0 : num));
													formData.daily_summary_cron = `0 ${dailySummaryMinute} ${dailySummaryHour} * * *`;
												}}
											>
												{#each Array.from({ length: 24 }, (_, i) => i) as h}
													<option value={String(h).padStart(2, '0')}>
														{String(h).padStart(2, '0')}
													</option>
												{/each}
											</select>
											<span class="text-lg font-medium">:</span>
											<select
												id="daily-summary-minute"
												class="border-input bg-background ring-offset-background h-10 rounded-md border px-3 py-2 text-sm focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-2 focus-visible:outline-none"
												bind:value={dailySummaryMinuteInput}
												on:change={(e) => {
													const value = e.currentTarget.value;
													dailySummaryMinuteInput = value;
													const num = parseInt(value, 10);
													dailySummaryMinute = Math.max(0, Math.min(59, isNaN(num) ? 0 : num));
													formData.daily_summary_cron = `0 ${dailySummaryMinute} ${dailySummaryHour} * * *`;
												}}
											>
												{#each Array.from({ length: 60 }, (_, i) => i) as m}
													<option value={String(m).padStart(2, '0')}>
														{String(m).padStart(2, '0')}
													</option>
												{/each}
											</select>
										</div>
										<p class="text-muted-foreground text-sm">
											设置每日汇总消息的发送时间（24小时制，格式：HH:MM）
										</p>
									</div>
									<Separator />
								{/if}
								<div class="space-y-2">
									<Label for="notification-interval">消息队列等待时间（秒）</Label>
									<Input
										id="notification-interval"
										type="number"
										min="1"
										max="60"
										bind:value={formData.notification_interval}
									/>
									<p class="text-muted-foreground text-sm">
										每条消息发送后等待的时间，建议范围：3-10秒，默认5秒
									</p>
								</div>
								{#if formData.notifiers && formData.notifiers.length > 0}
									<Separator />
									<div class="flex items-center justify-between">
										<div class="space-y-0.5">
											<Label for="enable-quiet-hours">静默时间段</Label>
											<p class="text-muted-foreground text-sm">
												在指定时间段内，统计通知将延迟到静默结束后发送
											</p>
										</div>
										<Switch
											id="enable-quiet-hours"
											bind:checked={formData.enable_notification_quiet_hours}
										/>
									</div>
									{#if formData.enable_notification_quiet_hours}
										<div class="rounded-lg border p-4 bg-muted/50">
											<div class="flex flex-wrap items-start gap-8">
												<div class="space-y-2">
												<Label for="quiet-hours-start">静默开始时间（小时）</Label>
												<select
													id="quiet-hours-start"
													class="border-input bg-background ring-offset-background h-10 rounded-md border px-3 py-2 text-sm focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-2 focus-visible:outline-none"
													bind:value={quietHoursStartInput}
													on:change={(e) => {
														const value = e.currentTarget.value;
														quietHoursStartInput = value;
														const num = parseInt(value, 10);
														formData.quiet_hours_start = Math.max(0, Math.min(23, isNaN(num) ? 0 : num));
													}}
												>
													{#each Array.from({ length: 24 }, (_, i) => i) as h}
														<option value={String(h).padStart(2, '0')}>
															{String(h).padStart(2, '0')}
														</option>
													{/each}
												</select>
												<p class="text-muted-foreground text-sm">
													静默开始时间（0-23，例如 22 表示晚上10点）
												</p>
												</div>
												<div class="space-y-2">
												<Label for="quiet-hours-end">静默结束时间（小时）</Label>
												<select
													id="quiet-hours-end"
													class="border-input bg-background ring-offset-background h-10 rounded-md border px-3 py-2 text-sm focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-2 focus-visible:outline-none"
													bind:value={quietHoursEndInput}
													on:change={(e) => {
														const value = e.currentTarget.value;
														quietHoursEndInput = value;
														const num = parseInt(value, 10);
														formData.quiet_hours_end = Math.max(0, Math.min(23, isNaN(num) ? 0 : num));
													}}
												>
													{#each Array.from({ length: 24 }, (_, i) => i) as h}
														<option value={String(h).padStart(2, '0')}>
															{String(h).padStart(2, '0')}
														</option>
													{/each}
												</select>
												<p class="text-muted-foreground text-sm">
													静默结束时间（0-23，例如 09 表示早上9点）。如果结束时间小于开始时间，表示跨天（如 22:00-09:00）
												</p>
												</div>
											</div>
										</div>
									{/if}
								{/if}
							</div>
						</div>

						<!-- 通知器管理 -->
						<div class="space-y-4">
							<div class="flex items-center justify-between">
								<div>
									<h3 class="text-lg font-semibold">通知器管理</h3>
									<p class="text-muted-foreground text-sm">
										配置通知器，在下载任务出现错误时发送通知
									</p>
								</div>
								<Button onclick={openAddNotifierDialog}>+ 添加通知器</Button>
							</div>

						{#if !formData.notifiers || formData.notifiers.length === 0}
							<div class="rounded-lg border-2 border-dashed py-12 text-center">
								<p class="text-muted-foreground">暂无通知器配置</p>
								<Button class="mt-4" variant="outline" onclick={openAddNotifierDialog}>
									添加第一个通知器
								</Button>
							</div>
						{:else}
							<div class="space-y-3">
								{#each formData.notifiers as notifier, index (index)}
									<div class="flex items-center justify-between rounded-lg border p-4">
										<div class="flex-1">
											{#if notifier.type === 'telegram'}
												<div class="flex items-center gap-2">
													<Badge variant="secondary">Telegram</Badge>
													<span class="text-muted-foreground text-sm">
														Chat ID: {notifier.chat_id}
													</span>
												</div>
											{:else if notifier.type === 'webhook'}
												<div class="flex items-center gap-2">
													<Badge variant="secondary">Webhook</Badge>
													<span class="text-muted-foreground text-sm">
														{notifier.url}
													</span>
												</div>
											{/if}
										</div>
										<div class="flex gap-2">
											<Button
												size="sm"
												variant="outline"
												onclick={() => openEditNotifierDialog(notifier, index)}
											>
												编辑
											</Button>
											<Button size="sm" variant="secondary" onclick={() => testNotifier(notifier)}>
												测试
											</Button>
											<Button size="sm" variant="destructive" onclick={() => deleteNotifier(index)}>
												删除
											</Button>
										</div>
									</div>
								{/each}
							</div>
						{/if}
					</div>
				</div>
			</Tabs.Content>

				<!-- 高级设置 -->
				<Tabs.Content value="advanced" class="mt-6 space-y-6">
					<div class="grid grid-cols-1 gap-6 lg:grid-cols-2">
						<div class="space-y-2">
							<Label for="video-concurrent">视频并发数</Label>
							<Input
								id="video-concurrent"
								type="number"
								min="1"
								max="20"
								bind:value={formData.concurrent_limit.video}
							/>
						</div>
						<div class="space-y-2">
							<Label for="page-concurrent">分页并发数</Label>
							<Input
								id="page-concurrent"
								type="number"
								min="1"
								max="20"
								bind:value={formData.concurrent_limit.page}
							/>
						</div>
						<div class="space-y-2">
							<Label for="nfo-time-type">NFO时间类型</Label>
							<select
								id="nfo-time-type"
								class="border-input bg-background ring-offset-background placeholder:text-muted-foreground focus-visible:ring-ring flex h-10 w-full rounded-md border px-3 py-2 text-sm file:border-0 file:bg-transparent file:text-sm file:font-medium focus-visible:ring-2 focus-visible:ring-offset-2 focus-visible:outline-none disabled:cursor-not-allowed disabled:opacity-50"
								bind:value={formData.nfo_time_type}
							>
								<option value="favtime">收藏时间</option>
								<option value="pubtime">发布时间</option>
							</select>
						</div>
					</div>

					<Separator />

					<div class="space-y-4">
						<div class="mb-4 flex items-center space-x-2">
							<Switch
								id="rate-limit-enable"
								checked={formData.concurrent_limit.rate_limit !== null &&
									formData.concurrent_limit.rate_limit !== undefined}
								onCheckedChange={(checked) => {
									if (checked) {
										formData!.concurrent_limit.rate_limit = { limit: 4, duration: 250 };
									} else {
										formData!.concurrent_limit.rate_limit = undefined;
									}
								}}
							/>
							<Label for="rate-limit-enable">启用请求频率限制</Label>
						</div>
						{#if formData.concurrent_limit.rate_limit}
							<div class="grid grid-cols-1 gap-6 lg:grid-cols-2">
								<div class="space-y-2">
									<Label for="rate-limit-duration">时间间隔（毫秒）</Label>
									<Input
										id="rate-limit-duration"
										type="number"
										min="100"
										bind:value={formData.concurrent_limit.rate_limit.duration}
									/>
									<p class="text-muted-foreground text-xs">请求限制的时间窗口（毫秒）</p>
								</div>
								<div class="space-y-2">
									<Label for="rate-limit-limit">限制请求数</Label>
									<Input
										id="rate-limit-limit"
										type="number"
										min="1"
										bind:value={formData.concurrent_limit.rate_limit.limit}
									/>
									<p class="text-muted-foreground text-xs">每个时间间隔内允许的最大请求数</p>
								</div>
							</div>
						{/if}
					</div>

					<Separator />
					<div class="space-y-4">
						<div class="mb-4 flex items-center space-x-2">
							<Switch
								id="download-enable"
								bind:checked={formData.concurrent_limit.download.enable}
							/>
							<Label for="rate-limit-duration">启用单文件分块下载</Label>
						</div>
						<div class="grid grid-cols-1 gap-6 lg:grid-cols-2">
							<div class="space-y-2">
								<Label for="download-concurrency">下载分块数</Label>
								<Input
									id="download-concurrency"
									type="number"
									min="1"
									max="16"
									disabled={!formData.concurrent_limit.download.enable}
									bind:value={formData.concurrent_limit.download.concurrency}
								/>
								<p class="text-muted-foreground text-xs">
									单文件将分为若干大小相同的块并行下载，所有分块下载完毕后合并
								</p>
							</div>
							<div class="space-y-2">
								<Label for="download-threshold">启用分块下载的文件大小阈值（字节）</Label>
								<Input
									id="download-threshold"
									type="number"
									min="1048576"
									disabled={!formData.concurrent_limit.download?.enable}
									bind:value={formData.concurrent_limit.download.threshold}
								/>
								<p class="text-muted-foreground text-xs">
									大于该阈值的文件才使用分块下载，文件过小时分块下载的拆分合并成本可能大于带来的增益
								</p>
							</div>
						</div>
					</div>
				</Tabs.Content>
			</Tabs.Root>

			<div class="flex justify-end pt-6">
				<Button onclick={saveConfig} disabled={saving} size="lg">
					{saving ? '保存中...' : '保存配置'}
				</Button>
			</div>
		</div>
	{:else}
		<div class="flex items-center justify-center py-16">
			<div class="space-y-4 text-center">
				<p class="text-muted-foreground">请先进行前端认证以加载配置</p>
			</div>
		</div>
	{/if}
</div>

<Dialog.Root bind:open={showNotifierDialog}>
	<Dialog.Portal>
		<Dialog.Overlay class="bg-background/80 fixed inset-0 z-50 backdrop-blur-sm" />
		<Dialog.Content
			class="bg-background fixed top-[50%] left-[50%] z-50 grid w-full max-w-lg translate-x-[-50%] translate-y-[-50%] gap-4 border p-6 shadow-lg duration-200 sm:rounded-lg"
		>
			<Dialog.Header>
				<Dialog.Title>
					{isEditing ? '编辑通知器' : '添加通知器'}
				</Dialog.Title>
				<Dialog.Description>配置通知器类型和参数</Dialog.Description>
			</Dialog.Header>

			{#if showNotifierDialog}
				<NotifierDialog
					notifier={editingNotifier}
					onSave={(notifier) => {
						if (isEditing && editingNotifierIndex !== null) {
							updateNotifier(editingNotifierIndex, notifier);
						} else {
							addNotifier(notifier);
						}
					}}
					onCancel={closeNotifierDialog}
				/>
			{/if}
		</Dialog.Content>
	</Dialog.Portal>
</Dialog.Root>

<!-- QR 登录弹窗 -->
<Dialog.Root bind:open={showQrLoginDialog}>
	<Dialog.Portal>
		<Dialog.Overlay class="bg-background/80 fixed inset-0 z-50 backdrop-blur-sm" />
		<Dialog.Content
			class="bg-background fixed top-[50%] left-[50%] z-50 grid w-full max-w-md translate-x-[-50%] translate-y-[-50%] gap-4 border p-6 shadow-lg duration-200 sm:rounded-lg"
		>
			<Dialog.Header>
				<Dialog.Title>扫码登录</Dialog.Title>
				<Dialog.Description>使用哔哩哔哩 APP 扫描二维码登录</Dialog.Description>
			</Dialog.Header>

			<QrLogin bind:this={qrLoginComponent} onSuccess={handleQrLoginSuccess} />
		</Dialog.Content>
	</Dialog.Portal>
</Dialog.Root>

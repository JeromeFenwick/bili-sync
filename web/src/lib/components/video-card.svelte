<script lang="ts">
	import { Badge } from '$lib/components/ui/badge/index.js';
	import { Card, CardContent, CardHeader, CardTitle } from '$lib/components/ui/card/index.js';
	import { Button } from '$lib/components/ui/button/index.js';
	import * as AlertDialog from '$lib/components/ui/alert-dialog/index.js';
	import * as DropdownMenu from '$lib/components/ui/dropdown-menu/index.js';
	import { Checkbox } from '$lib/components/ui/checkbox/index.js';
	import { Label } from '$lib/components/ui/label/index.js';
	import type { VideoInfo } from '$lib/types';
	import RotateCcwIcon from '@lucide/svelte/icons/rotate-ccw';
	import BrushCleaningIcon from '@lucide/svelte/icons/brush-cleaning';
	import UserIcon from '@lucide/svelte/icons/user';
	import SquareArrowOutUpRightIcon from '@lucide/svelte/icons/square-arrow-out-up-right';
	import { goto } from '$app/navigation';
	import * as Tooltip from '$lib/components/ui/tooltip/index.js';
	import api from '$lib/api';
	import { toast } from 'svelte-sonner';
	import type { ApiError } from '$lib/types';
	import { configStore } from '$lib/stores/config';
	import { onMount } from 'svelte';

	// 将 bvid 设置为可选属性，但保留 VideoInfo 的其它所有属性
	export let video: Omit<VideoInfo, 'bvid'> & { bvid?: string };
	export let showActions: boolean = true; // 控制是否显示操作按钮
	export let mode: 'default' | 'detail' | 'page' = 'default'; // 卡片模式
	export let customTitle: string = ''; // 自定义标题
	export let customSubtitle: string = ''; // 自定义副标题
	export let taskNames: string[] = []; // 自定义任务名称
	export let showProgress: boolean = true; // 是否显示进度信息
	export let onReset: ((forceReset: boolean) => Promise<void>) | null = null; // 自定义重置函数
	export let onClearAndReset: (() => Promise<void>) | null = null; // 自定义清空重置函数
	export let resetDialogOpen = false; // 导出对话框状态，让父组件可以控制
	export let clearAndResetDialogOpen = false; // 导出清空重置对话框状态
	export let resetting = false;
	export let clearAndResetting = false;
	export let onRetry: ((videoId: number, taskIndex: number, isPage: boolean) => Promise<void>) | null = null; // 自定义重试函数
	export let isSelectionMode: boolean = false; // 是否处于批量编辑模式
	export let isSelected: boolean = false; // 是否被选中
	export let onToggleSelection: (() => void) | null = null; // 切换选择状态的回调

	let forceReset = false;
	let retryingTaskIndex: number | null = null; // 正在重试的任务索引
	let retryConfirmDialogOpen = false; // 重试确认对话框状态
	let pendingRetryTaskIndex: number | null = null; // 待确认重试的任务索引
	let retryWarningDialogOpen = false; // 重试警告对话框状态
	let enableCoverBackground = false; // 是否启用封面背景渲染
	let moreMenuOpen = false; // 更多菜单打开状态

	function getStatusText(status: number): string {
		if (status === 7) {
			return '已完成';
		} else if (status === 0) {
			return '未开始';
		} else {
			return `失败${status}次`;
		}
	}

	function getSegmentColor(status: number): string {
		if (status === 7) {
			return 'bg-emerald-500';
		} else if (status === 0) {
			return 'bg-yellow-500';
		} else {
			return 'bg-rose-500';
		}
	}

	function getOverallStatus(
		downloadStatus: number[],
		shouldDownload: boolean,
		isPaidVideo: boolean
	): {
		text: string;
		style: string;
	} {
		if (isPaidVideo) {
			// 收费视频，显示为"收费"
			return { text: '收费', style: 'bg-yellow-600 text-yellow-100' };
		}
		if (!shouldDownload) {
			// 被过滤规则排除，显示为"跳过"
			return { text: '跳过', style: 'bg-gray-100 text-gray-700' };
		}
		const completed = downloadStatus.filter((status) => status === 7).length;
		const total = downloadStatus.length;
		const failed = downloadStatus.filter((status) => status !== 7 && status !== 0).length;

		if (completed === total) {
			// 全部完成，显示为"完成"
			return { text: '完成', style: 'bg-emerald-700 text-emerald-100' };
		} else if (failed > 0) {
			// 出现了失败，显示为"失败"
			return { text: '失败', style: 'bg-rose-700 text-rose-100' };
		} else {
			// 还未开始，显示为"等待"
			return { text: '等待', style: 'bg-yellow-700 text-yellow-100' };
		}
	}

	function getTaskName(index: number): string {
		if (taskNames.length > 0) {
			return taskNames[index] || `任务${index + 1}`;
		}
		const defaultTaskNames = ['视频封面', '视频信息', 'UP主头像', 'UP主信息', '分页下载'];
		return defaultTaskNames[index] || `任务${index + 1}`;
	}

	$: overallStatus = getOverallStatus(video.download_status, video.should_download, video.is_paid_video);
	$: completed = video.download_status.filter((status) => status === 7).length;
	$: total = video.download_status.length;

	async function handleReset() {
		resetting = true;
		if (onReset) {
			await onReset(forceReset);
		}
		resetting = false;
		resetDialogOpen = false;
		forceReset = false;
	}

	function checkTaskStatus(taskIndex: number): {
		canRetry: boolean;
		needConfirm: boolean;
		needWarning: boolean;
		message: string;
	} {
		const status = video.download_status[taskIndex];
		const isPaidVideo = video.is_paid_video;
		const shouldDownload = video.should_download;

		// 已完成（status === 7）：显示确认提示
		if (status === 7) {
			return {
				canRetry: true,
				needConfirm: true,
				needWarning: false,
				message: `该任务「${getTaskName(taskIndex)}」已完成，确定要重新下载吗？`
			};
		}

		// 跳过/收费：显示警告提示
		if (!shouldDownload || isPaidVideo) {
			const reason = isPaidVideo ? '收费视频' : '已跳过';
			return {
				canRetry: true,
				needConfirm: false,
				needWarning: true,
				message: `该任务「${getTaskName(taskIndex)}」为${reason}，重试后定时任务仍会跳过`
			};
		}

		// 未开始（status === 0）或失败（status > 0 && status < 7）：直接重试，无需确认
		return {
			canRetry: true,
			needConfirm: false,
			needWarning: false,
			message: ''
		};
	}

	function handleRetryTaskClick(taskIndex: number) {
		if (retryingTaskIndex !== null) {
			return; // 正在重试中，忽略新的点击
		}

		const statusCheck = checkTaskStatus(taskIndex);
		
		if (!statusCheck.canRetry) {
			toast.info(statusCheck.message);
			return;
		}

		if (statusCheck.needConfirm) {
			// 需要确认
			pendingRetryTaskIndex = taskIndex;
			retryConfirmDialogOpen = true;
		} else if (statusCheck.needWarning) {
			// 需要警告
			pendingRetryTaskIndex = taskIndex;
			retryWarningDialogOpen = true;
		} else {
			// 直接重试
			executeRetryTask(taskIndex);
		}
	}

	async function executeRetryTask(taskIndex: number) {
		if (retryingTaskIndex !== null) {
			return; // 正在重试中，忽略新的点击
		}
		retryingTaskIndex = taskIndex;
		try {
			if (onRetry) {
				// 使用自定义重试函数
				await onRetry(video.id, taskIndex, mode === 'page');
			} else {
				// 默认重试逻辑
				if (mode === 'page') {
					// 分页任务
					const result = await api.retryPageTask(video.id, { task_index: taskIndex });
					if (result.data.success) {
						toast.success(`已触发重试：${getTaskName(taskIndex)}`);
						// 更新本地状态
						video.download_status = result.data.video.download_status;
					} else {
						toast.error('重试失败');
					}
				} else {
					// 视频任务
					const result = await api.retryVideoTask(video.id, { task_index: taskIndex });
					if (result.data.success) {
						toast.success(`已触发重试：${getTaskName(taskIndex)}`);
						// 更新本地状态
						video.download_status = result.data.video.download_status;
					} else {
						toast.error('重试失败');
					}
				}
			}
		} catch (error) {
			console.error('重试任务失败：', error);
			toast.error('重试任务失败', {
				description: (error as ApiError).message
			});
		} finally {
			retryingTaskIndex = null;
		}
	}

	async function handleRetryConfirm() {
		if (pendingRetryTaskIndex !== null) {
			await executeRetryTask(pendingRetryTaskIndex);
			pendingRetryTaskIndex = null;
		}
		retryConfirmDialogOpen = false;
	}

	async function handleRetryWarningConfirm() {
		if (pendingRetryTaskIndex !== null) {
			await executeRetryTask(pendingRetryTaskIndex);
			pendingRetryTaskIndex = null;
		}
		retryWarningDialogOpen = false;
	}

	async function handleClearAndReset() {
		clearAndResetting = true;
		if (onClearAndReset) {
			await onClearAndReset();
		}
		clearAndResetting = false;
		clearAndResetDialogOpen = false;
	}

	function handleViewDetail() {
		goto(`/video/${video.id}`);
	}

	function handleCoverClick(e: MouseEvent) {
		e.stopPropagation(); // 阻止事件冒泡，避免触发 handleCardClick
		handleViewDetail();
	}

	// 根据模式确定显示的标题和副标题
	$: displayTitle = customTitle || video.name;
	$: displaySubtitle = customSubtitle || video.upper_name;
	$: cardClasses =
		mode === 'default'
			? `group flex min-w-0 flex-col transition-all hover:shadow-lg hover:shadow-primary/5 border-border/50 aspect-[16/9] ${
					isSelectionMode ? (isSelected ? 'ring-2 ring-primary' : 'cursor-pointer') : ''
				}`
			: `transition-all hover:shadow-lg border-border/50 ${
					isSelectionMode ? (isSelected ? 'ring-2 ring-primary' : 'cursor-pointer') : ''
				}`;

	function handleCardClick(e: MouseEvent) {
		// 在批量编辑模式下，点击卡片切换选择状态
		if (isSelectionMode && onToggleSelection) {
			// 检查点击的目标是否是按钮或链接，如果是则不触发选择切换
			const target = e.target as HTMLElement;
			if (
				target.closest('button') ||
				target.closest('a') ||
				target.closest('[role="button"]') ||
				target.closest('.dropdown-menu') ||
				target.closest('.alert-dialog')
			) {
				return;
			}
			onToggleSelection();
		}
	}

	// 加载配置
	onMount(() => {
		const unsubscribe = configStore.subscribe((config) => {
			enableCoverBackground = config?.enable_cover_background ?? false;
		});
		return unsubscribe;
	});

	// 计算卡片样式，支持封面背景
	$: cardWrapperClasses = enableCoverBackground && video.cover
		? 'relative overflow-hidden'
		: '';
</script>

<Card class="{cardClasses} p-0 overflow-hidden relative" onclick={handleCardClick}>
	{#if enableCoverBackground && video.cover}
		<!-- 封面容器 -->
		<div 
			class="relative h-full w-full overflow-hidden rounded-xl cursor-pointer transition-opacity hover:opacity-90"
			onclick={handleCoverClick}
			role="button"
			tabindex="0"
			onkeydown={(e) => {
				if (e.key === 'Enter' || e.key === ' ') {
					e.preventDefault();
					handleCoverClick(e as any);
				}
			}}
		>
			<img
				src={video.cover}
				alt=""
				referrerPolicy="no-referrer"
				class="absolute inset-0 w-full h-full object-cover"
				loading="lazy"
			/>
			<!-- 状态徽章 -->
			<div class="absolute top-0 right-0 z-20 p-3">
				{#if showActions}
					<DropdownMenu.Root bind:open={moreMenuOpen}>
						<DropdownMenu.Trigger>
							{#snippet child({ props })}
								<Badge
									{...props}
									variant="secondary"
									class="shrink-0 px-2 py-1 text-xs font-medium {overallStatus.style} cursor-pointer transition-opacity hover:opacity-80"
									onclick={(e) => {
										e.stopPropagation(); // 阻止事件冒泡，避免触发 handleCardClick
									}}
								>
									{overallStatus.text}
								</Badge>
							{/snippet}
						</DropdownMenu.Trigger>
						<DropdownMenu.Content align="end" class="w-48">
							<DropdownMenu.Item class="cursor-pointer" onclick={() => { resetDialogOpen = true; moreMenuOpen = false; }}>
								<RotateCcwIcon class="mr-2 h-4 w-4" />
								重置
							</DropdownMenu.Item>
							<DropdownMenu.Item
								class="cursor-pointer"
								onclick={() => { clearAndResetDialogOpen = true; moreMenuOpen = false; }}
							>
								<BrushCleaningIcon class="mr-2 h-4 w-4" />
								清空重置
							</DropdownMenu.Item>
							<DropdownMenu.Item
								class="cursor-pointer"
								onclick={() => {
									window.open(`https://www.bilibili.com/video/${video.bvid}/`, '_blank');
									moreMenuOpen = false;
								}}
							>
								<SquareArrowOutUpRightIcon class="mr-2 h-4 w-4" />
								在 B 站打开
							</DropdownMenu.Item>
						</DropdownMenu.Content>
					</DropdownMenu.Root>
				{:else}
					<Badge
						variant="secondary"
						class="shrink-0 px-2 py-1 text-xs font-medium {overallStatus.style}"
					>
						{overallStatus.text}
					</Badge>
				{/if}
			</div>
			<!-- UP主名称 - 浮动在底部，带渐变背景 -->
			{#if displaySubtitle}
				<div class="absolute bottom-0 left-0 right-0 z-20 flex min-w-0 items-center gap-1 px-2 pb-1.5">
					<!-- 黑色渐变效果 - 模仿B站官方风格 -->
					<div class="absolute inset-0 bg-gradient-to-t from-black/80 via-black/30 to-black/0"></div>
					<!-- UP主名称内容 -->
					<div class="relative z-10 flex min-w-0 items-center gap-1 text-sm text-white">
						<UserIcon class="h-3.5 w-3.5 shrink-0" />
						<span class="min-w-0 cursor-default truncate" title={displaySubtitle}>
							{displaySubtitle}
						</span>
					</div>
				</div>
			{/if}
		</div>
	{:else}
		<!-- 没有封面时显示状态徽章和UP主名称 -->
		<div class="relative flex flex-1 flex-col z-10">
			<CardHeader class="relative z-10 shrink-0 pb-3">
				<div class="flex min-w-0 items-start justify-end gap-3">
					{#if showActions}
						<DropdownMenu.Root bind:open={moreMenuOpen}>
							<DropdownMenu.Trigger>
								{#snippet child({ props })}
									<Badge
										{...props}
										variant="secondary"
										class="shrink-0 px-2 py-1 text-xs font-medium {overallStatus.style} cursor-pointer transition-opacity hover:opacity-80"
										onclick={(e) => {
											e.stopPropagation(); // 阻止事件冒泡，避免触发 handleCardClick
										}}
									>
										{overallStatus.text}
									</Badge>
								{/snippet}
							</DropdownMenu.Trigger>
							<DropdownMenu.Content align="end" class="w-48">
								<DropdownMenu.Item class="cursor-pointer" onclick={() => { resetDialogOpen = true; moreMenuOpen = false; }}>
									<RotateCcwIcon class="mr-2 h-4 w-4" />
									重置
								</DropdownMenu.Item>
								<DropdownMenu.Item
									class="cursor-pointer"
									onclick={() => { clearAndResetDialogOpen = true; moreMenuOpen = false; }}
								>
									<BrushCleaningIcon class="mr-2 h-4 w-4" />
									清空重置
								</DropdownMenu.Item>
								<DropdownMenu.Item
									class="cursor-pointer"
									onclick={() => {
										window.open(`https://www.bilibili.com/video/${video.bvid}/`, '_blank');
										moreMenuOpen = false;
									}}
								>
									<SquareArrowOutUpRightIcon class="mr-2 h-4 w-4" />
									在 B 站打开
								</DropdownMenu.Item>
							</DropdownMenu.Content>
						</DropdownMenu.Root>
					{:else}
						<Badge
							variant="secondary"
							class="shrink-0 px-2 py-1 text-xs font-medium {overallStatus.style}"
						>
							{overallStatus.text}
						</Badge>
					{/if}
				</div>
			</CardHeader>
			{#if displaySubtitle}
				<div class="absolute bottom-0 left-0 z-10 flex min-w-0 items-center gap-1 px-2 pb-1.5 text-muted-foreground text-sm">
					<UserIcon class="h-3.5 w-3.5 shrink-0" />
					<span class="min-w-0 cursor-default truncate" title={displaySubtitle}>
						{displaySubtitle}
					</span>
				</div>
			{/if}
		</div>
	{/if}
	
	<!-- 详情页和分页模式：标题和进度条在卡片内部 -->
	{#if (mode === 'detail' || mode === 'page')}
		<CardContent class="space-y-1.5">
			<!-- 视频标题 -->
			<div class="truncate text-sm font-medium" title={displayTitle}>
				{displayTitle}
			</div>
			<!-- 进度条区域 -->
			{#if showProgress && video.download_status && video.download_status.length > 0}
				<div class="space-y-1">
					<!-- 进度信息 -->
					<div class="text-muted-foreground flex justify-between text-xs font-medium">
						<span class="truncate">下载进度</span>
						<span class="shrink-0">{completed}/{total}</span>
					</div>
					<!-- 进度条 -->
					<div class="flex w-full gap-0.5">
						{#each video.download_status as status, index (index)}
							<Tooltip.Root>
								<Tooltip.Trigger class="flex-1">
									<div
										class="h-1.5 w-full rounded-full transition-all {getSegmentColor(
											status
										)} {retryingTaskIndex === index
											? 'opacity-50 cursor-wait'
											: 'cursor-pointer hover:opacity-80'}"
										onclick={(e) => {
											e.stopPropagation();
											handleRetryTaskClick(index);
										}}
										role="button"
										tabindex="0"
										onkeydown={(e) => {
											if (e.key === 'Enter' || e.key === ' ') {
												e.preventDefault();
												handleRetryTaskClick(index);
											}
										}}
									></div>
								</Tooltip.Trigger>
								<Tooltip.Content>
									<p class="text-xs">
										{getTaskName(index)}: {getStatusText(status)}
										{retryingTaskIndex === index ? ' (重试中...)' : ' (点击重试)'}
									</p>
								</Tooltip.Content>
							</Tooltip.Root>
						{/each}
					</div>
				</div>
			{/if}
		</CardContent>
	{/if}
</Card>

<!-- 默认模式：标题和进度条移出卡片 -->
{#if mode === 'default'}
	<div class="mt-2 space-y-1.5">
		<!-- 视频标题 -->
		<div class="truncate text-sm font-medium" title={displayTitle}>
			{displayTitle}
		</div>
		<!-- 进度条区域 -->
		{#if showProgress && video.download_status && video.download_status.length > 0}
			<div class="space-y-1">
				<!-- 进度信息 -->
				<div class="text-muted-foreground flex justify-between text-xs font-medium">
					<span class="truncate">下载进度</span>
					<span class="shrink-0">{completed}/{total}</span>
				</div>
				<!-- 进度条 -->
				<div class="flex w-full gap-0.5">
					{#each video.download_status as status, index (index)}
						<Tooltip.Root>
							<Tooltip.Trigger class="flex-1">
								<div
									class="h-1.5 w-full rounded-full transition-all {getSegmentColor(
										status
									)} {retryingTaskIndex === index
										? 'opacity-50 cursor-wait'
										: 'cursor-pointer hover:opacity-80'}"
									onclick={(e) => {
										e.stopPropagation();
										handleRetryTaskClick(index);
									}}
									role="button"
									tabindex="0"
									onkeydown={(e) => {
										if (e.key === 'Enter' || e.key === ' ') {
											e.preventDefault();
											handleRetryTaskClick(index);
										}
									}}
								></div>
							</Tooltip.Trigger>
							<Tooltip.Content>
								<p class="text-xs">
									{getTaskName(index)}: {getStatusText(status)}
									{retryingTaskIndex === index ? ' (重试中...)' : ' (点击重试)'}
								</p>
							</Tooltip.Content>
						</Tooltip.Root>
					{/each}
				</div>
			</div>
		{/if}

	</div>
{/if}

<!-- 重置确认对话框 -->
<AlertDialog.Root bind:open={resetDialogOpen}>
	<AlertDialog.Content>
		<AlertDialog.Header>
			<AlertDialog.Title>重置视频</AlertDialog.Title>
			<AlertDialog.Description>
				确定要重置视频 <strong>"{displayTitle}"</strong> 的下载状态吗？
				<br />
				此操作会将所有的失败状态重置为未开始，<span class="text-destructive font-medium"
					>无法撤销</span
				>。
			</AlertDialog.Description>
		</AlertDialog.Header>

		<div class="space-y-4 py-4">
			<div class="rounded-lg border border-orange-200 bg-orange-50 p-3">
				<div class="mb-2 flex items-center space-x-2">
					<Checkbox id="force-reset-all" bind:checked={forceReset} />
					<Label for="force-reset-all" class="text-sm font-medium text-orange-700"
						>⚠️ 强制重置</Label
					>
				</div>
				<p class="text-xs leading-relaxed text-orange-700">
					除重置失败状态外还会检查修复任务状态的标识位 <br />
					版本升级引入新任务时勾选该选项进行重置，可以允许旧视频执行新任务
				</p>
			</div>
		</div>

		<AlertDialog.Footer>
			<AlertDialog.Cancel
				onclick={() => {
					forceReset = false;
				}}>取消</AlertDialog.Cancel
			>
			<AlertDialog.Action
				onclick={handleReset}
				disabled={resetting}
				class={forceReset ? 'bg-orange-600 hover:bg-orange-700' : ''}
			>
				{resetting ? '重置中...' : forceReset ? '确认强制重置' : '确认重置'}
			</AlertDialog.Action>
		</AlertDialog.Footer>
	</AlertDialog.Content>
</AlertDialog.Root>

<!-- 清空重置确认对话框 -->
<AlertDialog.Root bind:open={clearAndResetDialogOpen}>
	<AlertDialog.Content>
		<AlertDialog.Header>
			<AlertDialog.Title>清空重置视频</AlertDialog.Title>
			<AlertDialog.Description>
				确定要清空重置视频 <strong>"{displayTitle}"</strong> 吗？
				<br />
				<br />
				此操作会：
				<ul class="mt-2 ml-4 list-disc space-y-1">
					<li>将视频状态重置为未开始</li>
					<li>删除所有分页信息</li>
					<li class="text-destructive font-medium">删除视频对应的文件夹</li>
				</ul>
				<br />
				该功能可在多页视频变更后手动触发全量更新，执行后<span class="text-destructive font-medium"
					>无法撤销</span
				>。
			</AlertDialog.Description>
		</AlertDialog.Header>

		<AlertDialog.Footer>
			<AlertDialog.Cancel>取消</AlertDialog.Cancel>
			<AlertDialog.Action
				onclick={handleClearAndReset}
				disabled={clearAndResetting}
				class="bg-destructive hover:bg-destructive/90"
			>
				{clearAndResetting ? '清空重置中...' : '确认清空重置'}
			</AlertDialog.Action>
		</AlertDialog.Footer>
	</AlertDialog.Content>
</AlertDialog.Root>

<!-- 重试确认对话框（已完成的任务） -->
<AlertDialog.Root bind:open={retryConfirmDialogOpen}>
	<AlertDialog.Content>
		<AlertDialog.Header>
			<AlertDialog.Title>确认重试</AlertDialog.Title>
			<AlertDialog.Description>
				{#if pendingRetryTaskIndex !== null}
					{checkTaskStatus(pendingRetryTaskIndex).message}
				{/if}
			</AlertDialog.Description>
		</AlertDialog.Header>

		<AlertDialog.Footer>
			<AlertDialog.Cancel>取消</AlertDialog.Cancel>
			<AlertDialog.Action
				onclick={handleRetryConfirm}
				disabled={retryingTaskIndex !== null}
				class="bg-primary hover:bg-primary/90"
			>
				{retryingTaskIndex !== null ? '重试中...' : '确认重试'}
			</AlertDialog.Action>
		</AlertDialog.Footer>
	</AlertDialog.Content>
</AlertDialog.Root>

<!-- 重试警告对话框（跳过/收费的任务） -->
<AlertDialog.Root bind:open={retryWarningDialogOpen}>
	<AlertDialog.Content>
		<AlertDialog.Header>
			<AlertDialog.Title>警告</AlertDialog.Title>
			<AlertDialog.Description>
				{#if pendingRetryTaskIndex !== null}
					{checkTaskStatus(pendingRetryTaskIndex).message}
				{/if}
			</AlertDialog.Description>
		</AlertDialog.Header>

		<AlertDialog.Footer>
			<AlertDialog.Cancel>取消</AlertDialog.Cancel>
			<AlertDialog.Action
				onclick={handleRetryWarningConfirm}
				disabled={retryingTaskIndex !== null}
				class="bg-yellow-600 hover:bg-yellow-700 text-white"
			>
				{retryingTaskIndex !== null ? '重试中...' : '仍要重试'}
			</AlertDialog.Action>
		</AlertDialog.Footer>
	</AlertDialog.Content>
</AlertDialog.Root>

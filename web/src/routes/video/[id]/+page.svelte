<script lang="ts">
	import { page } from '$app/stores';
	import { goto } from '$app/navigation';
	import { onMount } from 'svelte';
	import { Button } from '$lib/components/ui/button/index.js';
	import api from '$lib/api';
	import SquareArrowOutUpRightIcon from '@lucide/svelte/icons/square-arrow-out-up-right';
	import type { ApiError, VideoResponse, UpdateVideoStatusRequest } from '$lib/types';
	import RotateCcwIcon from '@lucide/svelte/icons/rotate-ccw';
	import EditIcon from '@lucide/svelte/icons/edit';
	import BrushCleaningIcon from '@lucide/svelte/icons/brush-cleaning';
	import UserIcon from '@lucide/svelte/icons/user';
	import { setBreadcrumb } from '$lib/stores/breadcrumb';
	import { appStateStore, ToQuery } from '$lib/stores/filter';
	import VideoCard from '$lib/components/video-card.svelte';
	import StatusEditor from '$lib/components/status-editor.svelte';
	import { toast } from 'svelte-sonner';
	import { Badge } from '$lib/components/ui/badge/index.js';
	import * as Tooltip from '$lib/components/ui/tooltip/index.js';
	import * as AlertDialog from '$lib/components/ui/alert-dialog/index.js';
	import { Checkbox } from '$lib/components/ui/checkbox/index.js';
	import { Label } from '$lib/components/ui/label/index.js';

	let videoData: VideoResponse | null = null;
	let loading = false;
	let error: string | null = null;
	let resetDialogOpen = false;
	let resetting = false;
	let clearAndResetDialogOpen = false;
	let clearAndResetting = false;
	let statusEditorOpen = false;
	let statusEditorLoading = false;
	let forceReset = false;
	let titleFontSize = 'clamp(1.5rem, 4vw + 0.5rem, 3.75rem)'; // 默认字体大小

	async function loadVideoDetail() {
		const videoId = parseInt($page.params.id);
		if (isNaN(videoId)) {
			error = '无效的视频 ID';
			toast.error('无效的视频 ID');
			return;
		}
		loading = true;
		error = null;
		try {
			const result = await api.getVideo(videoId);
			videoData = result.data;
		} catch (error) {
			console.error('加载视频详情失败：', error);
			toast.error('加载视频详情失败', {
				description: (error as ApiError).message
			});
		} finally {
			loading = false;
		}
	}

	onMount(() => {
		setBreadcrumb([
			{
				label: '视频',
				href: `/${ToQuery($appStateStore)}`
			},
			{ label: '视频详情' }
		]);
	});

	// 监听路由参数变化
	$: if ($page.params.id) {
		loadVideoDetail();
	}

	async function handleStatusEditorSubmit(request: UpdateVideoStatusRequest) {
		if (!videoData) return;

		statusEditorLoading = true;
		try {
			const result = await api.updateVideoStatus(videoData.video.id, request);
			const data = result.data;

			if (data.success) {
				// 更新本地数据
				videoData = {
					video: data.video,
					pages: data.pages
				};
				statusEditorOpen = false;
				toast.success('状态更新成功');
			} else {
				toast.error('状态更新失败');
			}
		} catch (error) {
			console.error('状态更新失败：', error);
			toast.error('状态更新失败', {
				description: (error as ApiError).message
			});
		} finally {
			statusEditorLoading = false;
		}
	}

	async function handleReset() {
		if (!videoData) return;
		resetting = true;
		try {
			const result = await api.resetVideoStatus(videoData.video.id, { force: forceReset });
			const data = result.data;
			if (data.resetted) {
				videoData = {
					video: data.video,
					pages: data.pages
				};
				toast.success('重置成功');
			} else {
				toast.info('重置无效', {
					description: `视频「${data.video.name}」没有失败的状态，无需重置`
				});
			}
		} catch (error) {
			console.error('重置失败:', error);
			toast.error('重置失败', {
				description: (error as ApiError).message
			});
		} finally {
			resetting = false;
			resetDialogOpen = false;
			forceReset = false;
		}
	}

	async function handleRetryTask(videoId: number, taskIndex: number, isPage: boolean) {
		if (!videoData) return;
		try {
			if (isPage) {
				// 分页任务
				const result = await api.retryPageTask(videoId, { task_index: taskIndex });
				if (result.data.success) {
					// 重新加载视频详情
					await loadVideoDetail();
					toast.success(`已触发重试：${isPage ? '分页' : '视频'}任务`);
				} else {
					toast.error('重试失败');
				}
			} else {
				// 视频任务
				const result = await api.retryVideoTask(videoId, { task_index: taskIndex });
				if (result.data.success) {
					// 重新加载视频详情
					await loadVideoDetail();
					toast.success(`已触发重试：${isPage ? '分页' : '视频'}任务`);
				} else {
					toast.error('重试失败');
				}
			}
		} catch (error) {
			console.error('重试任务失败：', error);
			toast.error('重试任务失败', {
				description: (error as ApiError).message
			});
		}
	}

	async function handleClearAndReset() {
		if (!videoData) return;
		clearAndResetting = true;
		try {
			const result = await api.clearAndResetVideoStatus(videoData.video.id);
			const data = result.data;
			videoData = {
				video: data.video,
				pages: []
			};
			if (data.warning) {
				toast.warning('清空重置成功', {
					description: data.warning
				});
			} else {
				toast.success('清空重置成功', {
					description: `视频「${data.video.name}」已清空重置`
				});
			}
		} catch (error) {
			console.error('清空重置失败：', error);
			toast.error('清空重置失败', {
				description: (error as ApiError).message
			});
		} finally {
			clearAndResetting = false;
			clearAndResetDialogOpen = false;
		}
	}

	// 计算下载进度
	$: completed = videoData?.video.download_status.filter((status) => status === 7).length ?? 0;
	$: total = videoData?.video.download_status.length ?? 0;

	function getSegmentColor(status: number): string {
		if (status === 7) {
			return 'bg-emerald-500';
		} else if (status === 0) {
			return 'bg-yellow-500';
		} else {
			return 'bg-rose-500';
		}
	}

	function getStatusText(status: number): string {
		if (status === 7) {
			return '已完成';
		} else if (status === 0) {
			return '未开始';
		} else {
			return `失败${status}次`;
		}
	}

	function getTaskName(index: number): string {
		const taskNames = ['视频封面', '视频信息', 'UP主头像', 'UP主信息', '分页下载'];
		return taskNames[index] || `任务${index + 1}`;
	}

	// 分页任务名称
	function getPageTaskName(index: number): string {
		const taskNames = ['视频封面', '视频内容', '视频信息', '视频弹幕', '视频字幕'];
		return taskNames[index] || `任务${index + 1}`;
	}

	// 计算分页的完成状态
	function getPageCompleted(pageStatus: number[]): number {
		return pageStatus.filter((status) => status === 7).length;
	}

	function getPageTotal(pageStatus: number[]): number {
		return pageStatus.length;
	}

	// 根据标题长度动态计算字体大小
	function calculateTitleFontSize(title: string): string {
		if (!title) return 'clamp(1.5rem, 4vw + 0.5rem, 3.75rem)';
		
		const length = title.length;
		// 根据字符数动态调整字体大小，使用clamp确保响应式
		// 短标题（<30字符）：使用大字体 clamp(1.5rem, 4vw + 0.5rem, 3.75rem)
		// 中等标题（30-60字符）：使用中等字体 clamp(1.25rem, 3vw + 0.5rem, 2.5rem)
		// 长标题（60-100字符）：使用较小字体 clamp(1rem, 2.5vw + 0.5rem, 1.875rem)
		// 超长标题（>100字符）：使用最小字体 clamp(0.875rem, 2vw + 0.5rem, 1.5rem)
		
		if (length < 30) {
			return 'clamp(1.5rem, 4vw + 0.5rem, 3.75rem)';
		} else if (length < 60) {
			return 'clamp(1.25rem, 3vw + 0.5rem, 2.5rem)';
		} else if (length < 100) {
			return 'clamp(1rem, 2.5vw + 0.5rem, 1.875rem)';
		} else {
			return 'clamp(0.875rem, 2vw + 0.5rem, 1.5rem)';
		}
	}

	// 当视频数据更新时，重新计算字体大小
	$: if (videoData?.video.name) {
		titleFontSize = calculateTitleFontSize(videoData.video.name);
	}
</script>

<svelte:head>
	<title>{videoData?.video.name || '视频详情'} - Bili Sync</title>
</svelte:head>

{#if loading}
	<div class="flex items-center justify-center py-12">
		<div class="text-muted-foreground">加载中...</div>
	</div>
{:else if error}
	<div class="flex items-center justify-center py-12">
		<div class="space-y-2 text-center">
			<p class="text-destructive">{error}</p>
			<button
				class="text-muted-foreground hover:text-foreground text-sm transition-colors"
				onclick={() => goto('/')}
			>
				返回首页
			</button>
		</div>
	</div>
{:else if videoData}
	<!-- Netflix 风格视频详情页 -->
	<div class="relative min-h-screen">
		<!-- 背景图片区域 -->
		{#if videoData.video.cover}
			<div class="fixed inset-0 z-0 pointer-events-none">
				<img
					src={videoData.video.cover}
					alt={videoData.video.name}
					referrerPolicy="no-referrer"
					class="h-full w-full object-cover blur-md"
					loading="eager"
				/>
				<!-- 深色渐变遮罩 - Netflix 风格 -->
				<div
					class="absolute inset-0 bg-gradient-to-b from-black/10 via-black/20 to-black/30 dark:from-black/20 dark:via-black/30 dark:to-black/40"
				></div>
				<!-- 底部渐变遮罩，确保内容可读 -->
				<div
					class="absolute bottom-0 left-0 right-0 h-96 bg-gradient-to-t from-background via-background/20 to-transparent"
				></div>
			</div>
		{/if}

		<!-- 内容区域 - 所有内容贴近屏幕下方 -->
		<div class="relative z-10 flex min-h-screen flex-col justify-end pb-24 pointer-events-auto">
			<div class="container mx-auto w-full px-4 pb-4 lg:px-8">
				<!-- 主内容区域 - Netflix 风格 -->
				<div class="mb-8 space-y-6">
					<!-- 视频封面 -->
					{#if videoData.video.cover}
						<div class="w-full">
							<div
								class="relative aspect-[16/9] overflow-hidden rounded-lg shadow-2xl cursor-pointer transition-transform hover:scale-[1.02]"
								onclick={() =>
									window.open(`https://www.bilibili.com/video/${videoData.video.bvid}/`, '_blank')}
								role="button"
								tabindex="0"
								onkeydown={(e) => {
									if (e.key === 'Enter' || e.key === ' ') {
										e.preventDefault();
										window.open(`https://www.bilibili.com/video/${videoData.video.bvid}/`, '_blank');
									}
								}}
							>
								<img
									src={videoData.video.cover}
									alt={videoData.video.name}
									referrerPolicy="no-referrer"
									class="h-full w-full object-cover"
									loading="eager"
								/>
							</div>
						</div>
					{/if}

					<!-- 标题和操作区域 -->
					<div class="flex flex-col gap-6 lg:flex-row lg:items-end lg:justify-between lg:gap-12">
						<!-- 左侧：标题和UP主信息 -->
						<div class="flex-1 space-y-4 text-slate-900 dark:text-foreground lg:max-w-2xl">
							<!-- 视频标题 -->
							<h1
								class="font-bold leading-tight break-words"
								style="font-size: {titleFontSize};"
							>
								{videoData.video.name}
							</h1>

							<!-- UP主信息 -->
							{#if videoData.video.upper_name}
								<div class="flex items-center gap-2 text-lg text-slate-800 dark:text-foreground/80">
									<UserIcon class="h-5 w-5" />
									<span>{videoData.video.upper_name}</span>
								</div>
							{/if}
						</div>

						<!-- 右侧：下载进度和操作按钮 -->
						<div class="flex flex-col gap-4 text-slate-900 dark:text-foreground lg:min-w-[300px] lg:justify-end lg:ml-auto">
							<!-- 下载进度 -->
							{#if videoData.video.download_status && videoData.video.download_status.length > 0}
								<div class="space-y-3">
									<div class="flex items-center justify-between text-sm font-medium text-slate-800 dark:text-foreground/80">
										<span>下载进度</span>
										<span>{completed}/{total}</span>
									</div>
									<!-- 进度条 -->
									<div class="flex w-full gap-0.5">
										{#each videoData.video.download_status as status, index (index)}
											<Tooltip.Root>
												<Tooltip.Trigger class="flex-1">
													<div
														class="h-2 w-full rounded-full transition-all {getSegmentColor(
															status
														)} cursor-pointer hover:opacity-80"
														role="button"
														tabindex="0"
													></div>
												</Tooltip.Trigger>
												<Tooltip.Content>
													<p class="text-xs">
														{getTaskName(index)}: {getStatusText(status)}
													</p>
												</Tooltip.Content>
											</Tooltip.Root>
										{/each}
									</div>
								</div>
							{/if}

							<!-- 操作按钮组 -->
							<div class="flex flex-wrap gap-3">
								<Button
									size="lg"
									class="bg-white text-slate-900 hover:bg-white/90 dark:bg-foreground dark:text-background dark:hover:bg-foreground/90"
									onclick={() =>
										window.open(`https://www.bilibili.com/video/${videoData.video.bvid}/`, '_blank')}
								>
									<SquareArrowOutUpRightIcon class="mr-2 h-5 w-5" />
									在 B 站打开
								</Button>
								<Button
									size="lg"
									variant="outline"
									class="border-slate-300/50 bg-white/60 text-slate-800 backdrop-blur-sm hover:bg-white/80 dark:border-foreground/20 dark:bg-background/80 dark:text-foreground dark:hover:bg-background/90"
									onclick={() => (statusEditorOpen = true)}
									disabled={statusEditorLoading}
								>
									<EditIcon class="mr-2 h-5 w-5" />
									编辑状态
								</Button>
								<Button
									size="lg"
									variant="outline"
									class="border-slate-300/50 bg-white/60 text-slate-800 backdrop-blur-sm hover:bg-white/80 dark:border-foreground/20 dark:bg-background/80 dark:text-foreground dark:hover:bg-background/90"
									onclick={() => (resetDialogOpen = true)}
									disabled={resetting || clearAndResetting}
								>
									<RotateCcwIcon class="mr-2 h-5 w-5 {resetting ? 'animate-spin' : ''}" />
									重置
								</Button>
								<Button
									size="lg"
									variant="outline"
									class="border-slate-300/50 bg-white/60 text-slate-800 backdrop-blur-sm hover:bg-white/80 dark:border-foreground/20 dark:bg-background/80 dark:text-foreground dark:hover:bg-background/90"
									onclick={() => (clearAndResetDialogOpen = true)}
									disabled={resetting || clearAndResetting}
								>
									<BrushCleaningIcon class="mr-2 h-5 w-5 {clearAndResetting ? 'animate-spin' : ''}" />
									清空重置
								</Button>
							</div>
						</div>
					</div>
				</div>

				<!-- 分页列表区域 - 贴着底部，无背景 -->
				<div class="relative z-10">
					{#if videoData.pages && videoData.pages.length > 0}
						<div class="space-y-6">
							<div class="flex items-center justify-between">
								<h2 class="text-2xl font-bold text-slate-900 dark:text-foreground">分页列表</h2>
								<div class="text-slate-700 dark:text-foreground/60 text-sm">
									共 {videoData.pages.length} 个分页
								</div>
							</div>

							<div
								class="grid gap-4"
								style="grid-template-columns: repeat(auto-fill, minmax(300px, 1fr));"
							>
								{#each videoData.pages as pageInfo (pageInfo.id)}
									{@const pageCompleted = getPageCompleted(pageInfo.download_status)}
									{@const pageTotal = getPageTotal(pageInfo.download_status)}
									<!-- 现代化分页卡片 -->
									<div
										class="group relative overflow-hidden rounded-2xl border border-slate-200/50 bg-white/70 p-6 backdrop-blur-md transition-all hover:border-slate-300/70 hover:bg-white/90 hover:shadow-2xl dark:border-foreground/10 dark:bg-background/20 dark:hover:border-foreground/20 dark:hover:bg-background/30"
									>
										<!-- 标题和状态 -->
										<div class="mb-4 flex items-start justify-between gap-3">
											<h3
												class="flex-1 truncate text-base font-semibold text-slate-900 dark:text-foreground"
												title="P{pageInfo.pid}: {pageInfo.name}"
											>
												P{pageInfo.pid}: {pageInfo.name}
											</h3>
											<Badge
												variant="secondary"
												class="shrink-0 px-3 py-1 text-xs font-medium {pageCompleted === pageTotal
													? 'bg-emerald-700 text-emerald-100'
													: pageInfo.download_status.some((s) => s !== 7 && s !== 0)
														? 'bg-rose-700 text-rose-100'
														: 'bg-yellow-700 text-yellow-100'}"
											>
												{pageCompleted === pageTotal
													? '完成'
													: pageInfo.download_status.some((s) => s !== 7 && s !== 0)
														? '失败'
														: '等待'}
											</Badge>
										</div>

										<!-- 下载进度 -->
										{#if pageInfo.download_status && pageInfo.download_status.length > 0}
											<div class="space-y-3">
												<div class="flex items-center justify-between text-xs font-medium text-slate-700 dark:text-foreground/70">
													<span>下载进度</span>
													<span class="text-slate-800 dark:text-foreground/80">{pageCompleted}/{pageTotal}</span>
												</div>
												<!-- 进度条 -->
												<div class="flex w-full gap-1">
													{#each pageInfo.download_status as status, index (index)}
														<Tooltip.Root>
															<Tooltip.Trigger class="flex-1">
																<div
																	class="h-2 w-full rounded-full transition-all {getSegmentColor(
																		status
																	)} cursor-pointer hover:opacity-80"
																	role="button"
																	tabindex="0"
																	onclick={(e) => {
																		e.stopPropagation();
																		handleRetryTask(pageInfo.id, index, true);
																	}}
																	onkeydown={(e) => {
																		if (e.key === 'Enter' || e.key === ' ') {
																			e.preventDefault();
																			handleRetryTask(pageInfo.id, index, true);
																		}
																	}}
																></div>
															</Tooltip.Trigger>
															<Tooltip.Content>
																<p class="text-xs">
																	{getPageTaskName(index)}: {getStatusText(status)}
																	{status === 7 ? '' : ' (点击重试)'}
																</p>
															</Tooltip.Content>
														</Tooltip.Root>
													{/each}
												</div>
											</div>
										{/if}
									</div>
								{/each}
							</div>
						</div>
					{:else}
						<div class="py-12 text-center">
							<div class="space-y-2">
								<p class="text-slate-700 dark:text-foreground/60">暂无分 P 数据</p>
							</div>
						</div>
					{/if}
				</div>
			</div>
		</div>
	</div>

	<!-- 状态编辑器 -->
	{#if videoData}
		<StatusEditor
			bind:open={statusEditorOpen}
			video={videoData.video}
			pages={videoData.pages}
			loading={statusEditorLoading}
			onsubmit={handleStatusEditorSubmit}
		/>
	{/if}

	<!-- 重置确认对话框 -->
	<AlertDialog.Root bind:open={resetDialogOpen}>
		<AlertDialog.Content>
			<AlertDialog.Header>
				<AlertDialog.Title>重置视频</AlertDialog.Title>
				<AlertDialog.Description>
					确定要重置视频 <strong>"{videoData?.video.name || ''}"</strong> 的下载状态吗？
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
					确定要清空重置视频 <strong>"{videoData?.video.name || ''}"</strong> 吗？
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
{/if}

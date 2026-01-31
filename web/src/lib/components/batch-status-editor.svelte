<script lang="ts">
	import { Button } from '$lib/components/ui/button/index.js';
	import {
		Sheet,
		SheetContent,
		SheetDescription,
		SheetFooter,
		SheetHeader,
		SheetTitle
	} from '$lib/components/ui/sheet/index.js';
	import type { StatusUpdate, UpdateFilteredVideoStatusRequest } from '$lib/types';
	import { toast } from 'svelte-sonner';
	import { Switch } from '$lib/components/ui/switch/index.js';
	import { Label } from '$lib/components/ui/label/index.js';
	import DollarSignIcon from '@lucide/svelte/icons/dollar-sign';

	let {
		open = $bindable(false),
		videoCount = 0,
		loading = false,
		onsubmit
	}: {
		open?: boolean;
		videoCount?: number;
		loading?: boolean;
		onsubmit: (request: UpdateFilteredVideoStatusRequest) => void;
	} = $props();

	// 视频任务名称（与后端 VideoStatus 对应）
	const videoTaskNames = ['视频封面', '视频信息', 'UP 主头像', 'UP 主信息', '分页下载'];

	// 初始状态：所有任务都是 null（未设置）
	let videoStatuses = $state<(number | null)[]>(Array(5).fill(null));
	let isPaidVideo = $state<boolean | null>(null); // null = 不修改，true/false = 设置值

	// 重置单个视频任务
	function resetVideoTask(taskIndex: number) {
		videoStatuses[taskIndex] = null;
	}

	function handleVideoStatusChange(taskIndex: number, newValue: number | null) {
		videoStatuses[taskIndex] = newValue;
	}

	function resetAllStatuses() {
		videoStatuses = Array(5).fill(null);
		isPaidVideo = null;
	}

	function hasVideoChanges(): boolean {
		return videoStatuses.some((status) => status !== null);
	}

	function hasShouldDownloadChange(): boolean {
		return isPaidVideo !== null;
	}

	// 使用 $derived 创建派生状态
	let hasAnyChanges = $derived(hasVideoChanges() || hasShouldDownloadChange());

	function buildRequest(): UpdateFilteredVideoStatusRequest {
		const request: UpdateFilteredVideoStatusRequest = {};

		request.video_updates = [];
		videoStatuses.forEach((status, index) => {
			if (status !== null) {
				request.video_updates!.push({
					status_index: index,
					status_value: status
				});
			}
		});

		// 如果 is_paid_video 有变化，添加到请求中
		if (hasShouldDownloadChange()) {
			request.is_paid_video = isPaidVideo;
		}

		return request;
	}

	function handleSubmit() {
		if (!hasAnyChanges) {
			toast.info('没有状态变更需要提交');
			return;
		}
		const request = buildRequest();
		if (!request.video_updates?.length && request.is_paid_video === undefined) {
			toast.info('没有状态变更需要提交');
			return;
		}
		onsubmit(request);
	}

	// 当对话框打开时重置状态
	$effect(() => {
		if (open) {
			resetAllStatuses();
		}
	});
</script>

<Sheet bind:open>
	<SheetContent side="right" class="flex w-full flex-col sm:max-w-3xl">
		<SheetHeader class="px-6 pb-2">
			<SheetTitle class="text-lg">批量编辑状态</SheetTitle>
			<SheetDescription class="text-muted-foreground space-y-1 text-sm">
				<div>批量编辑 {videoCount} 个视频的下载状态。可将任意子任务状态修改为"未开始"或"已完成"。</div>
				<div class="leading-relaxed text-orange-600">
					⚠️ 仅当分页下载状态不是"已完成"时，程序才会尝试执行分页下载。
				</div>
			</SheetDescription>
		</SheetHeader>

		<div class="flex-1 overflow-y-auto px-6">
			<div class="space-y-6 py-2">
				<div>
					<h3 class="mb-4 text-base font-medium">视频状态</h3>
					<div class="bg-card rounded-lg border p-4">
						<div class="space-y-3">
							{#each videoTaskNames as taskName, index (index)}
								{@const currentValue = videoStatuses[index]}
								<div
									class="bg-background hover:bg-muted/30 flex items-center justify-between rounded-md border p-3 transition-colors {currentValue !== null
										? 'border-blue-200 ring-2 ring-blue-500/20'
										: ''}"
								>
									<div class="flex items-center gap-3">
										<div>
											<div class="flex items-center gap-2">
												<span class="text-sm font-medium">{taskName}</span>
												{#if currentValue !== null}
													<span class="hidden text-xs font-medium text-blue-600 sm:inline">已设置</span>
													<div class="h-2 w-2 rounded-full bg-blue-500 sm:hidden" title="已设置"></div>
												{/if}
											</div>
											{#if currentValue !== null}
												<div class="mt-0.5 flex items-center gap-1.5">
													<div class="h-1.5 w-1.5 rounded-full {currentValue === 7 ? 'bg-emerald-600' : 'bg-yellow-600'}"></div>
													<span class="text-xs {currentValue === 7 ? 'text-emerald-600' : 'text-yellow-600'}">
														{currentValue === 7 ? '已完成' : '未开始'}
													</span>
												</div>
											{:else}
												<div class="text-muted-foreground mt-0.5 text-xs">未设置（将不修改此任务）</div>
											{/if}
										</div>
									</div>
									<div class="flex gap-1.5">
										{#if currentValue !== null}
											<Button
												variant="ghost"
												size="sm"
												onclick={() => resetVideoTask(index)}
												disabled={loading}
												class="h-7 min-w-[60px] cursor-pointer px-3 text-xs text-gray-600 hover:bg-gray-100"
												title="取消设置"
											>
												取消
											</Button>
										{/if}
										<Button
											variant={currentValue === 0 ? 'default' : 'outline'}
											size="sm"
											onclick={() => handleVideoStatusChange(index, currentValue === 0 ? null : 0)}
											disabled={loading}
											class="h-7 min-w-[60px] cursor-pointer px-3 text-xs {currentValue === 0
												? 'border-yellow-600 bg-yellow-600 font-medium text-white hover:bg-yellow-700'
												: 'hover:border-yellow-400 hover:bg-yellow-50 hover:text-yellow-700'}"
										>
											未开始
										</Button>
										<Button
											variant={currentValue === 7 ? 'default' : 'outline'}
											size="sm"
											onclick={() => handleVideoStatusChange(index, currentValue === 7 ? null : 7)}
											disabled={loading}
											class="h-7 min-w-[60px] cursor-pointer px-3 text-xs {currentValue === 7
												? 'border-emerald-600 bg-emerald-600 font-medium text-white hover:bg-emerald-700'
												: 'hover:border-emerald-400 hover:bg-emerald-50 hover:text-emerald-700'}"
										>
											已完成
										</Button>
									</div>
								</div>
							{/each}
						</div>
					</div>
					<!-- 收费视频标记 -->
					<div class="bg-card mt-4 rounded-lg border p-4">
						<div class="flex items-center justify-between">
							<div class="flex items-center gap-3">
								<DollarSignIcon class="text-muted-foreground h-5 w-5" />
								<div>
									<Label for="should-download" class="text-sm font-medium">
										标记为收费视频
									</Label>
									<p class="text-muted-foreground text-xs">
										设为收费视频后，定时任务将跳过这些视频的下载
									</p>
								</div>
							</div>
							<Switch
								id="should-download"
								checked={isPaidVideo === true}
								onCheckedChange={(checked) => {
									// 如果取消勾选，设置为 null（不修改）
									// 如果勾选，设置为 true
									isPaidVideo = checked ? true : null;
								}}
								disabled={loading}
							/>
						</div>
						{#if isPaidVideo !== null}
							<div class="text-muted-foreground mt-2 text-xs">
								{isPaidVideo
									? '已标记为收费视频，定时任务将跳过这些视频'
									: '已取消收费视频标记，视频将正常下载'}
							</div>
						{/if}
					</div>
				</div>
			</div>
		</div>

		<SheetFooter class="bg-background flex gap-2 border-t px-6 pt-4">
			<Button
				variant="outline"
				onclick={resetAllStatuses}
				disabled={!hasAnyChanges}
				class="flex-1 cursor-pointer"
			>
				重置所有状态
			</Button>
			<Button
				onclick={handleSubmit}
				disabled={loading || !hasAnyChanges}
				class="flex-1 cursor-pointer"
			>
				{loading ? '提交中...' : '提交更改'}
			</Button>
		</SheetFooter>
	</SheetContent>
</Sheet>


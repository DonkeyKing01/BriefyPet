# BriefyPet Ver2.2

## 1. 历史文件删除
public/pets/xiao-cat.json 
public/pets/xiao-cat.png
可以覆盖进git提交就删了可以，没有用了，注意相关编译依赖可以去除

## 2. 个人兴趣各 module 输入框里的背景引导文案
### 第一行通用
“写下你关注的主题、想看的内容类型和优先条件”
各个module下一行分别为
### 科技
例如：AI Agent、编程工具、大模型产品更新；优先教程、测评和重要发布
### 社科
例如：社会心理、青年文化、科技与社会；优先研究解读、案例分析和关键观点
### 商业
例如：AI 创业、产品策略、行业趋势；优先深度分析、公司动态和市场变化
### 成长
例如：学习方法、时间管理、表达沟通；优先可执行建议、经验总结和高质量书单
### 新闻观点
例如：全球科技新闻、热点事件评论、产业政策变化；优先背景解读和多角度观点总结
### 娱乐
例如：电影、动画、游戏和流行文化；优先高质量推荐、口碑评价和新作信息
### 科学
例如：AI、认知科学、物理和前沿研究；优先通俗解读、重要论文和新发现
### 医学
例如：睡眠、营养、运动健康和心理健康；优先循证研究、科普解读和实用建议

## 3. LLM模型配置交互流程修改

### 先选服务商
用户先在下拉框里选一个模型服务商，比如 DeepSeek、GLM、Kimi、OpenAI、SiliconFlow等，以及自定义选项
### 再选模型
选中服务商后，模型列表联动变化，只显示这个服务商当前预设支持的主流模型；默认选中模型列表中第一个
### 只填当前服务商的 API Key
不再同时展示一堆：
DeepSeek API Key
GLM API Key
Kimi API Key
而是统一只有一个字段：
API Key
并在字段输入框背景提示里面告诉用户：
这里填写当前所选服务商的 API Key
### 支持自定义配置导入
用户在模型服务商下拉框中没有选择服务商而是选择自定义时展示
{
  "provider": "provider_name",
  "base_url": "baseurl",
  "api": "API协议",
  "api_key": "your-api-key-here",
  "model": {
    "id": "model_id",
    "name": "model_name"
  }
}
其中 provider/base_url/api_key/model.id 字段需要手动填入
api 协议类型选择 OpenAI Compatible / Anthropic Native / Gemini Native
### 服务商以及每个服务商模型id
每个模型厂商的 model id 和 api 协议你都再确认一下，确保每个都可用
{
  "DeepSeek": [
    "deepseek-chat",
    "deepseek-reasoner"
  ],
  "Qwen": [
    "qwen3.5-flash",
    "qwen3.6-plus",
    "qwen3-max"
  ],
  "MiniMax": [
    "MiniMax-M2.5-highspeed",
    "MiniMax-M2.5",
    "MiniMax-M2.7-highspeed",
    "MiniMax-M2.7"
  ],
  "GLM": [
    "glm-4.7-flashx",
    "glm-5-turbo",
    "glm-4.7",
    "glm-5.1"
  ],
  "Kimi": [
    "kimi-k2.5",
    "kimi-k2-thinking"
  ],
  "OpenAI": [
    "gpt-5.4-nano",
    "gpt-5.4-mini",
    "gpt-5.4"
  ],
  "Gemini": [
    "gemini-2.5-flash-lite",
    "gemini-2.5-flash",
    "gemini-2.5-pro"
  ],
  "Anthropic": [
    "claude-haiku-4-5-20251001",
    "claude-sonnet-4-6",
    "claude-opus-4-7"
  ]
}
### 注意以上都是用户输入框输入和下拉框选择，json格式只是演示不是让用户这样填导入

## 4. 自定义 RSS 流程修改
Module可以按现在的分类来
Bucket要按Module选择后对应展示
比如 Module：科学 Bucket：物理/化学/生物
避免某个Module下面的Bucket被导入其他的Module下面引起混乱
确保导入功能是可以生效的

## 5. 源池开关流程修改
现有Module名称点击字符可以展开，但交互引导不够完善
建议加上小三角引导用户可以点击展开
下面的Bucket也同理加上小三角，建议增加缩进区分信息层级


## 6. 帮助引导页面项目与联系页排版调整
项目与联系
项目源码：
https://github.com/DonkeyKing01/BriefyPet
开发者邮箱：
Qingyang Jin: jinqingyang01@sjtu.edu.cn
Yuecheng He: 24300680058@m.fudan.edu.cn
欢迎提交PR和联系我们

严格按这样排版


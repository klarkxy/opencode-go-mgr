use super::protocol::{
    ApiFormat, NamespaceToolMapping, ProtocolError, RequestPlan, encode_anthropic_thinking_block,
    encode_chat_reasoning, responses_id, unix_seconds,
};
use bytes::{Bytes, BytesMut};
use serde_json::{Map, Value, json};
use std::collections::{BTreeMap, BTreeSet};

const MAX_PENDING_SSE_BYTES: usize = 8 * 1024 * 1024;

pub(crate) struct StreamConverter {
    source: ApiFormat,
    target: ApiFormat,
    model: String,
    custom_tools: BTreeSet<String>,
    namespace_tools: BTreeMap<String, NamespaceToolMapping>,
    response_parallel_tool_calls: bool,
    response_tool_choice: Value,
    response_tools: Vec<Value>,
    pending: BytesMut,
    input: InputState,
    output: OutputState,
}

#[derive(Default)]
struct InputState {
    started: bool,
    terminal: bool,
    message_delta_seen: bool,
    next_block: usize,
    active: BTreeMap<usize, BlockKind>,
    text_block: Option<usize>,
    reasoning_block: Option<usize>,
    chat_tools: BTreeMap<u64, ChatTool>,
    response_tools: BTreeMap<u64, ResponseTool>,
    response_parts: BTreeMap<(u64, u64, bool), usize>,
    response_delta_seen: BTreeSet<(u64, u64, bool)>,
    anthropic_reasoning: BTreeMap<usize, Value>,
    pending_stop: Option<String>,
    usage: Usage,
    saw_tool: bool,
}

#[derive(Default)]
struct OutputState {
    terminal: bool,
    id: String,
    model: String,
    created_at: u64,
    usage: Usage,
    stop_reason: Option<String>,
    finish_emitted: bool,
    next_tool_index: u64,
    next_output_index: u64,
    sequence: u64,
    blocks: BTreeMap<usize, OutputBlock>,
}

#[derive(Clone, Default)]
struct Usage {
    seen: bool,
    input: u64,
    output: u64,
    cached: u64,
    cache_creation: u64,
}

#[derive(Clone)]
enum BlockKind {
    Text,
    Reasoning,
    Tool { id: String, name: String },
}

#[derive(Default)]
struct ChatTool {
    block: Option<usize>,
    id: String,
    name: String,
    pending_arguments: String,
    started: bool,
}

#[derive(Default)]
struct ResponseTool {
    block: Option<usize>,
    arguments_seen: bool,
}

struct OutputBlock {
    kind: BlockKind,
    content: String,
    tool_index: Option<u64>,
    output_index: Option<u64>,
    custom_input_emitted: usize,
    closed: bool,
}

enum PivotEvent {
    Start {
        id: String,
        model: String,
        usage: Usage,
    },
    BlockStart {
        index: usize,
        kind: BlockKind,
    },
    TextDelta {
        index: usize,
        text: String,
    },
    ReasoningDelta {
        index: usize,
        text: String,
    },
    SignatureDelta {
        index: usize,
        signature: String,
    },
    ArgumentsDelta {
        index: usize,
        arguments: String,
    },
    BlockStop {
        index: usize,
    },
    MessageDelta {
        stop_reason: String,
        usage: Usage,
    },
    Stop,
    Error {
        kind: String,
        message: String,
    },
}

impl StreamConverter {
    pub(crate) fn new(plan: &RequestPlan) -> Self {
        Self {
            source: plan.upstream,
            target: plan.client,
            model: plan.model.clone(),
            custom_tools: plan.custom_tools.iter().cloned().collect(),
            namespace_tools: plan
                .namespace_tools
                .iter()
                .cloned()
                .map(|mapping| (mapping.flattened.clone(), mapping))
                .collect(),
            response_parallel_tool_calls: plan.response_parallel_tool_calls,
            response_tool_choice: plan.response_tool_choice.clone(),
            response_tools: plan.response_tools.clone(),
            pending: BytesMut::new(),
            input: InputState::default(),
            output: OutputState::default(),
        }
    }

    pub(crate) fn process_chunk(&mut self, chunk: Bytes) -> Result<Vec<Bytes>, ProtocolError> {
        if self.source == self.target {
            if self.is_terminal() {
                return Ok(Vec::new());
            }
            if self.pending.len() + chunk.len() > MAX_PENDING_SSE_BYTES {
                self.pending.clear();
                return Err(ProtocolError::new("SSE event exceeds 8 MiB"));
            }
            self.pending.extend_from_slice(&chunk);
            let frames = drain_frames(&mut self.pending);
            let mut output = Vec::new();
            for frame in frames {
                if self.input.terminal {
                    break;
                }
                let passthrough = frame.clone();
                let _ = self.convert_frames(vec![frame])?;
                output.push(passthrough);
            }
            return Ok(output);
        }
        if self.pending.len() + chunk.len() > MAX_PENDING_SSE_BYTES {
            self.pending.clear();
            return Err(ProtocolError::new("SSE event exceeds 8 MiB"));
        }
        self.pending.extend_from_slice(&chunk);
        let frames = drain_frames(&mut self.pending);
        self.convert_frames(frames)
    }

    pub(crate) fn finish(&mut self) -> Result<Vec<Bytes>, ProtocolError> {
        if self.source == self.target {
            let mut frames = drain_frames(&mut self.pending);
            if !self.pending.is_empty() {
                frames.push(self.pending.split().freeze());
            }
            let mut output = Vec::new();
            for frame in frames {
                if self.input.terminal {
                    break;
                }
                let passthrough = frame.clone();
                let _ = self.convert_frames(vec![frame])?;
                output.push(passthrough);
            }
            if self.input.terminal {
                return Ok(output);
            }
            return match self.source {
                ApiFormat::ChatCompletions if self.input.pending_stop.is_some() => {
                    self.input.terminal = true;
                    self.output.terminal = true;
                    output.push(done_frame());
                    Ok(output)
                }
                ApiFormat::Messages if self.input.message_delta_seen => {
                    self.input.terminal = true;
                    self.output.terminal = true;
                    output.push(sse_json(
                        Some("message_stop"),
                        &json!({"type":"message_stop"}),
                    ));
                    Ok(output)
                }
                _ => Err(ProtocolError::new(
                    "upstream SSE ended before a terminal event",
                )),
            };
        }

        let mut frames = drain_frames(&mut self.pending);
        if !self.pending.is_empty() {
            frames.push(self.pending.split().freeze());
        }
        let mut output = self.convert_frames(frames)?;
        if !self.input.terminal {
            if self.input.pending_stop.is_none() && !self.input.message_delta_seen {
                return Err(ProtocolError::new(
                    "upstream SSE ended before a terminal event",
                ));
            }
            let events = self.finish_input();
            output.extend(self.encode_all(events)?);
        }
        Ok(output)
    }

    pub(crate) fn error_event(&self, message: &str) -> Vec<Bytes> {
        if self.is_terminal() {
            return Vec::new();
        }
        match self.target {
            ApiFormat::Messages => vec![sse_json(
                Some("error"),
                &json!({"type":"error","error":{"type":"api_error","message":message}}),
            )],
            ApiFormat::ChatCompletions => vec![
                sse_json(
                    None,
                    &json!({"error":{"type":"server_error","message":message}}),
                ),
                done_frame(),
            ],
            ApiFormat::Responses => vec![sse_json(
                Some("response.failed"),
                &json!({
                    "type":"response.failed",
                    "sequence_number":self.output.sequence,
                    "response":self.failed_response_object("server_error", message)
                }),
            )],
            ApiFormat::Gemini => vec![sse_json(
                None,
                &json!({
                    "error":{"code":500,"message":message,"status":"INTERNAL"}
                }),
            )],
        }
    }

    pub(crate) fn is_terminal(&self) -> bool {
        self.input.terminal || self.output.terminal
    }

    fn convert_frames(&mut self, frames: Vec<Bytes>) -> Result<Vec<Bytes>, ProtocolError> {
        let mut output = Vec::new();
        for frame in frames {
            if self.input.terminal {
                break;
            }
            let Some((event_name, payload)) = parse_sse_frame(&frame)? else {
                continue;
            };
            let payload = payload.trim();
            let events = if payload.is_empty() {
                Vec::new()
            } else if payload == "[DONE]" {
                self.finish_input()
            } else {
                let value: Value = serde_json::from_str(payload)
                    .map_err(|e| ProtocolError::new(format!("invalid SSE JSON: {e}")))?;
                match self.source {
                    ApiFormat::Messages => self.decode_messages(value),
                    ApiFormat::ChatCompletions => self.decode_chat(value),
                    ApiFormat::Responses => self.decode_responses(event_name.as_deref(), value),
                    ApiFormat::Gemini => {
                        return Err(ProtocolError::new("Gemini is a client-only stream format"));
                    }
                }
            };
            output.extend(self.encode_all(events)?);
        }
        Ok(output)
    }

    fn encode_all(&mut self, events: Vec<PivotEvent>) -> Result<Vec<Bytes>, ProtocolError> {
        let mut output = Vec::new();
        for event in events {
            output.extend(match self.target {
                ApiFormat::Messages => self.encode_messages(event),
                ApiFormat::ChatCompletions => self.encode_chat(event),
                ApiFormat::Responses => self.encode_responses(event),
                ApiFormat::Gemini => self.encode_gemini(event)?,
            });
        }
        Ok(output)
    }

    fn start_if_needed(&mut self, value: &Value) -> Vec<PivotEvent> {
        if self.input.started {
            return Vec::new();
        }
        self.input.started = true;
        let id = value
            .pointer("/message/id")
            .or_else(|| value.pointer("/response/id"))
            .or_else(|| value.get("id"))
            .and_then(Value::as_str)
            .unwrap_or("msg_ocg")
            .to_string();
        let model = value
            .pointer("/message/model")
            .or_else(|| value.pointer("/response/model"))
            .or_else(|| value.get("model"))
            .and_then(Value::as_str)
            .unwrap_or(&self.model)
            .to_string();
        vec![PivotEvent::Start {
            id,
            model,
            usage: self.input.usage.clone(),
        }]
    }

    fn next_block(&mut self, kind: BlockKind) -> (usize, PivotEvent) {
        let index = self.input.next_block;
        self.input.next_block += 1;
        self.input.active.insert(index, kind.clone());
        (index, PivotEvent::BlockStart { index, kind })
    }

    fn finish_input(&mut self) -> Vec<PivotEvent> {
        if self.input.terminal {
            return Vec::new();
        }
        let mut events = Vec::new();

        let pending_tools: Vec<u64> = self
            .input
            .chat_tools
            .iter()
            .filter_map(|(index, tool)| (!tool.started).then_some(*index))
            .collect();
        for tool_index in pending_tools {
            events.extend(self.start_chat_tool(tool_index));
        }

        let indexes: Vec<usize> = self.input.active.keys().copied().collect();
        for index in indexes {
            self.input.active.remove(&index);
            events.push(PivotEvent::BlockStop { index });
        }
        if !self.input.message_delta_seen {
            self.input.message_delta_seen = true;
            events.push(PivotEvent::MessageDelta {
                stop_reason: self.input.pending_stop.clone().unwrap_or_else(|| {
                    if self.input.saw_tool {
                        "tool_use".to_string()
                    } else {
                        "end_turn".to_string()
                    }
                }),
                usage: self.input.usage.clone(),
            });
        }
        self.input.terminal = true;
        events.push(PivotEvent::Stop);
        events
    }

    fn decode_messages(&mut self, value: Value) -> Vec<PivotEvent> {
        let event_type = value.get("type").and_then(Value::as_str).unwrap_or("");
        match event_type {
            "message_start" => {
                self.input.started = true;
                self.input
                    .usage
                    .merge(anthropic_usage(value.pointer("/message/usage")));
                vec![PivotEvent::Start {
                    id: string_at(&value, "/message/id", "msg_ocg"),
                    model: string_at(&value, "/message/model", &self.model),
                    usage: self.input.usage.clone(),
                }]
            }
            "content_block_start" => {
                let index =
                    u64_at(&value, "/index").unwrap_or(self.input.next_block as u64) as usize;
                self.input.next_block = self.input.next_block.max(index + 1);
                let block = value.pointer("/content_block").unwrap_or(&Value::Null);
                let kind = match block.get("type").and_then(Value::as_str).unwrap_or("") {
                    "text" => BlockKind::Text,
                    "thinking" | "redacted_thinking" => {
                        self.input.anthropic_reasoning.insert(index, block.clone());
                        BlockKind::Reasoning
                    }
                    "tool_use" | "server_tool_use" => {
                        self.input.saw_tool = true;
                        BlockKind::Tool {
                            id: string_at(block, "/id", &format!("toolu_{index}")),
                            name: string_at(block, "/name", "tool"),
                        }
                    }
                    _ => return Vec::new(),
                };
                self.input.active.insert(index, kind.clone());
                vec![PivotEvent::BlockStart { index, kind }]
            }
            "content_block_delta" => {
                let index = u64_at(&value, "/index").unwrap_or(0) as usize;
                let delta = value.pointer("/delta").unwrap_or(&Value::Null);
                match delta.get("type").and_then(Value::as_str).unwrap_or("") {
                    "text_delta" => vec![PivotEvent::TextDelta {
                        index,
                        text: string_at(delta, "/text", ""),
                    }],
                    "thinking_delta" => {
                        let text = string_at(delta, "/thinking", "");
                        append_json_string(
                            self.input.anthropic_reasoning.get_mut(&index),
                            "thinking",
                            &text,
                        );
                        vec![PivotEvent::ReasoningDelta { index, text }]
                    }
                    "signature_delta" => {
                        let signature = string_at(delta, "/signature", "");
                        append_json_string(
                            self.input.anthropic_reasoning.get_mut(&index),
                            "signature",
                            &signature,
                        );
                        vec![PivotEvent::SignatureDelta { index, signature }]
                    }
                    "input_json_delta" => vec![PivotEvent::ArgumentsDelta {
                        index,
                        arguments: string_at(delta, "/partial_json", ""),
                    }],
                    _ => Vec::new(),
                }
            }
            "content_block_stop" => {
                let index = u64_at(&value, "/index").unwrap_or(0) as usize;
                self.input.active.remove(&index);
                vec![PivotEvent::BlockStop { index }]
            }
            "message_delta" => {
                self.input.message_delta_seen = true;
                let usage = anthropic_usage(value.get("usage"));
                self.input.usage.merge(usage);
                let stop_reason = string_at(
                    &value,
                    "/delta/stop_reason",
                    if self.input.saw_tool {
                        "tool_use"
                    } else {
                        "end_turn"
                    },
                );
                self.input.pending_stop = Some(stop_reason.clone());
                vec![PivotEvent::MessageDelta {
                    stop_reason,
                    usage: self.input.usage.clone(),
                }]
            }
            "message_stop" => self.finish_input(),
            "error" => {
                self.input.terminal = true;
                vec![PivotEvent::Error {
                    kind: string_at(&value, "/error/type", "api_error"),
                    message: string_at(&value, "/error/message", "upstream stream error"),
                }]
            }
            _ => Vec::new(),
        }
    }

    fn decode_chat(&mut self, value: Value) -> Vec<PivotEvent> {
        if let Some(error) = value.get("error") {
            self.input.terminal = true;
            return vec![PivotEvent::Error {
                kind: error
                    .get("type")
                    .or_else(|| error.get("code"))
                    .and_then(Value::as_str)
                    .unwrap_or("api_error")
                    .to_string(),
                message: error
                    .get("message")
                    .and_then(Value::as_str)
                    .unwrap_or("upstream stream error")
                    .to_string(),
            }];
        }

        self.input.usage.merge(chat_usage(value.get("usage")));
        let mut events = self.start_if_needed(&value);
        let Some(choices) = value.get("choices").and_then(Value::as_array) else {
            return events;
        };
        for choice in choices {
            let delta = choice.get("delta").unwrap_or(&Value::Null);
            let reasoning = delta
                .get("reasoning_content")
                .or_else(|| delta.get("reasoning"))
                .and_then(Value::as_str)
                .filter(|text| !text.is_empty());
            if let Some(text) = reasoning {
                self.close_chat_text_block(&mut events);
                let index = if let Some(index) = self.input.reasoning_block {
                    index
                } else {
                    let (index, start) = self.next_block(BlockKind::Reasoning);
                    self.input.reasoning_block = Some(index);
                    events.push(start);
                    index
                };
                events.push(PivotEvent::ReasoningDelta {
                    index,
                    text: text.to_string(),
                });
            }
            if let Some(text) = delta
                .get("content")
                .and_then(Value::as_str)
                .filter(|text| !text.is_empty())
            {
                self.close_chat_reasoning_block(&mut events);
                let index = if let Some(index) = self.input.text_block {
                    index
                } else {
                    let (index, start) = self.next_block(BlockKind::Text);
                    self.input.text_block = Some(index);
                    events.push(start);
                    index
                };
                events.push(PivotEvent::TextDelta {
                    index,
                    text: text.to_string(),
                });
            }
            if let Some(tool_calls) = delta.get("tool_calls").and_then(Value::as_array) {
                if !tool_calls.is_empty() {
                    self.close_chat_non_tool_blocks(&mut events);
                }
                for call in tool_calls {
                    let tool_index = call.get("index").and_then(Value::as_u64).unwrap_or(0);
                    let tool = self.input.chat_tools.entry(tool_index).or_default();
                    if let Some(id) = call.get("id").and_then(Value::as_str) {
                        tool.id = id.to_string();
                    }
                    if let Some(name) = call.pointer("/function/name").and_then(Value::as_str) {
                        tool.name = name.to_string();
                    }
                    let arguments = call
                        .pointer("/function/arguments")
                        .and_then(Value::as_str)
                        .unwrap_or("")
                        .to_string();
                    if tool.started {
                        if !arguments.is_empty() {
                            events.push(PivotEvent::ArgumentsDelta {
                                index: tool.block.expect("started tool has block"),
                                arguments,
                            });
                        }
                    } else {
                        tool.pending_arguments.push_str(&arguments);
                        if !tool.name.is_empty() {
                            events.extend(self.start_chat_tool(tool_index));
                        }
                    }
                }
            }
            if let Some(reason) = choice.get("finish_reason").and_then(Value::as_str) {
                self.input.pending_stop = Some(chat_stop_to_anthropic(reason).to_string());
            }
        }
        events
    }

    fn close_chat_text_block(&mut self, events: &mut Vec<PivotEvent>) {
        if let Some(index) = self.input.text_block.take() {
            self.input.active.remove(&index);
            events.push(PivotEvent::BlockStop { index });
        }
    }

    fn close_chat_reasoning_block(&mut self, events: &mut Vec<PivotEvent>) {
        if let Some(index) = self.input.reasoning_block.take() {
            self.input.active.remove(&index);
            events.push(PivotEvent::BlockStop { index });
        }
    }

    fn close_chat_non_tool_blocks(&mut self, events: &mut Vec<PivotEvent>) {
        self.close_chat_reasoning_block(events);
        self.close_chat_text_block(events);
    }

    fn start_chat_tool(&mut self, tool_index: u64) -> Vec<PivotEvent> {
        let Some(tool) = self.input.chat_tools.get_mut(&tool_index) else {
            return Vec::new();
        };
        if tool.started {
            return Vec::new();
        }
        if tool.id.is_empty() {
            tool.id = format!("call_{tool_index}");
        }
        if tool.name.is_empty() {
            tool.name = "tool".to_string();
        }
        let kind = BlockKind::Tool {
            id: tool.id.clone(),
            name: tool.name.clone(),
        };
        let index = self.input.next_block;
        self.input.next_block += 1;
        self.input.active.insert(index, kind.clone());
        self.input.saw_tool = true;
        tool.block = Some(index);
        tool.started = true;
        let arguments = std::mem::take(&mut tool.pending_arguments);
        let mut events = vec![PivotEvent::BlockStart { index, kind }];
        if !arguments.is_empty() {
            events.push(PivotEvent::ArgumentsDelta { index, arguments });
        }
        events
    }

    fn decode_responses(&mut self, event_name: Option<&str>, value: Value) -> Vec<PivotEvent> {
        let event_type = value
            .get("type")
            .and_then(Value::as_str)
            .or(event_name)
            .unwrap_or("");
        if event_type == "error" || event_type == "response.failed" {
            self.input.terminal = true;
            return vec![PivotEvent::Error {
                kind: value
                    .pointer("/response/error/code")
                    .or_else(|| value.pointer("/error/code"))
                    .or_else(|| value.pointer("/error/type"))
                    .and_then(Value::as_str)
                    .unwrap_or("api_error")
                    .to_string(),
                message: value
                    .pointer("/response/error/message")
                    .or_else(|| value.pointer("/error/message"))
                    .or_else(|| value.get("message"))
                    .and_then(Value::as_str)
                    .unwrap_or("upstream stream error")
                    .to_string(),
            }];
        }

        let mut events = self.start_if_needed(&value);
        match event_type {
            "response.created" | "response.in_progress" => {
                self.input
                    .usage
                    .merge(responses_usage(value.pointer("/response/usage")));
            }
            "response.output_item.added" => {
                let output_index = u64_at(&value, "/output_index").unwrap_or(0);
                if let Some(item) = value.get("item") {
                    if item.get("type").and_then(Value::as_str) == Some("function_call") {
                        events.extend(self.start_response_tool(output_index, item));
                    }
                }
            }
            "response.content_part.added" => {
                let output_index = u64_at(&value, "/output_index").unwrap_or(0);
                let content_index = u64_at(&value, "/content_index").unwrap_or(0);
                let part = value.get("part").unwrap_or(&Value::Null);
                let reasoning = matches!(
                    part.get("type").and_then(Value::as_str),
                    Some("reasoning_text" | "summary_text")
                );
                events.extend(self.start_response_part(output_index, content_index, reasoning));
            }
            "response.output_text.delta" | "response.refusal.delta" => {
                let output_index = u64_at(&value, "/output_index").unwrap_or(0);
                let content_index = u64_at(&value, "/content_index").unwrap_or(0);
                events.extend(self.start_response_part(output_index, content_index, false));
                let key = (output_index, content_index, false);
                self.input.response_delta_seen.insert(key);
                if let Some(index) = self.input.response_parts.get(&key).copied() {
                    events.push(PivotEvent::TextDelta {
                        index,
                        text: string_at(&value, "/delta", ""),
                    });
                }
            }
            "response.reasoning_text.delta" | "response.reasoning_summary_text.delta" => {
                let output_index = u64_at(&value, "/output_index").unwrap_or(0);
                let content_index = u64_at(&value, "/summary_index")
                    .or_else(|| u64_at(&value, "/content_index"))
                    .unwrap_or(0);
                events.extend(self.start_response_part(output_index, content_index, true));
                let key = (output_index, content_index, true);
                self.input.response_delta_seen.insert(key);
                if let Some(index) = self.input.response_parts.get(&key).copied() {
                    events.push(PivotEvent::ReasoningDelta {
                        index,
                        text: string_at(&value, "/delta", ""),
                    });
                }
            }
            "response.function_call_arguments.delta" => {
                let output_index = u64_at(&value, "/output_index").unwrap_or(0);
                if !self.input.response_tools.contains_key(&output_index) {
                    events.extend(self.start_response_tool(output_index, &value));
                }
                if let Some(tool) = self.input.response_tools.get_mut(&output_index) {
                    tool.arguments_seen = true;
                    if let Some(index) = tool.block {
                        events.push(PivotEvent::ArgumentsDelta {
                            index,
                            arguments: string_at(&value, "/delta", ""),
                        });
                    }
                }
            }
            "response.output_item.done" => {
                let output_index = u64_at(&value, "/output_index").unwrap_or(0);
                if let Some(item) = value.get("item") {
                    events.extend(self.complete_response_item(output_index, item));
                }
            }
            "response.completed" | "response.incomplete" => {
                self.input
                    .usage
                    .merge(responses_usage(value.pointer("/response/usage")));
                let status = value
                    .pointer("/response/status")
                    .and_then(Value::as_str)
                    .unwrap_or(if event_type == "response.incomplete" {
                        "incomplete"
                    } else {
                        "completed"
                    });
                self.input.pending_stop = Some(if status == "incomplete" {
                    match value
                        .pointer("/response/incomplete_details/reason")
                        .and_then(Value::as_str)
                    {
                        Some("content_filter") => "refusal".to_string(),
                        _ => "max_tokens".to_string(),
                    }
                } else if self.input.saw_tool {
                    "tool_use".to_string()
                } else {
                    "end_turn".to_string()
                });
                events.extend(self.finish_input());
            }
            _ => {}
        }
        events
    }

    fn start_response_part(
        &mut self,
        output_index: u64,
        content_index: u64,
        reasoning: bool,
    ) -> Vec<PivotEvent> {
        let key = (output_index, content_index, reasoning);
        if self.input.response_parts.contains_key(&key) {
            return Vec::new();
        }
        let kind = if reasoning {
            BlockKind::Reasoning
        } else {
            BlockKind::Text
        };
        let (index, start) = self.next_block(kind);
        self.input.response_parts.insert(key, index);
        vec![start]
    }

    fn start_response_tool(&mut self, output_index: u64, item: &Value) -> Vec<PivotEvent> {
        if self.input.response_tools.contains_key(&output_index) {
            return Vec::new();
        }
        let id = item
            .get("call_id")
            .or_else(|| item.get("id"))
            .and_then(Value::as_str)
            .unwrap_or("call_ocg")
            .to_string();
        let name = item
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or("tool")
            .to_string();
        let (index, start) = self.next_block(BlockKind::Tool {
            id: id.clone(),
            name: name.clone(),
        });
        self.input.saw_tool = true;
        self.input.response_tools.insert(
            output_index,
            ResponseTool {
                block: Some(index),
                arguments_seen: false,
            },
        );
        vec![start]
    }

    fn complete_response_item(&mut self, output_index: u64, item: &Value) -> Vec<PivotEvent> {
        let mut events = Vec::new();
        match item.get("type").and_then(Value::as_str).unwrap_or("") {
            "function_call" => {
                events.extend(self.start_response_tool(output_index, item));
                if let Some(tool) = self.input.response_tools.get(&output_index) {
                    if !tool.arguments_seen {
                        if let (Some(index), Some(arguments)) =
                            (tool.block, item.get("arguments").and_then(Value::as_str))
                        {
                            events.push(PivotEvent::ArgumentsDelta {
                                index,
                                arguments: arguments.to_string(),
                            });
                        }
                    }
                    if let Some(index) = tool.block {
                        self.input.active.remove(&index);
                        events.push(PivotEvent::BlockStop { index });
                    }
                }
            }
            "message" => {
                if let Some(parts) = item.get("content").and_then(Value::as_array) {
                    for (content_index, part) in parts.iter().enumerate() {
                        let kind = part.get("type").and_then(Value::as_str).unwrap_or("");
                        let reasoning = matches!(kind, "reasoning_text" | "summary_text");
                        let key = (output_index, content_index as u64, reasoning);
                        events.extend(self.start_response_part(
                            output_index,
                            content_index as u64,
                            reasoning,
                        ));
                        if !self.input.response_delta_seen.contains(&key) {
                            if let Some(index) = self.input.response_parts.get(&key).copied() {
                                let text = part
                                    .get("text")
                                    .or_else(|| part.get("refusal"))
                                    .and_then(Value::as_str)
                                    .unwrap_or("")
                                    .to_string();
                                events.push(if reasoning {
                                    PivotEvent::ReasoningDelta { index, text }
                                } else {
                                    PivotEvent::TextDelta { index, text }
                                });
                            }
                        }
                        if let Some(index) = self.input.response_parts.get(&key).copied() {
                            self.input.active.remove(&index);
                            events.push(PivotEvent::BlockStop { index });
                        }
                    }
                }
            }
            _ => {}
        }
        events
    }

    fn encode_messages(&mut self, event: PivotEvent) -> Vec<Bytes> {
        match event {
            PivotEvent::Start { id, model, usage } => vec![sse_json(
                Some("message_start"),
                &json!({
                    "type":"message_start",
                    "message":{"id":anthropic_id(&id),"type":"message","role":"assistant","content":[],"model":model,
                    "stop_reason":null,"stop_sequence":null,"usage":anthropic_usage_json(&usage)}
                }),
            )],
            PivotEvent::BlockStart { index, kind } => {
                let content_block = match kind {
                    BlockKind::Text => json!({"type":"text","text":""}),
                    BlockKind::Reasoning => json!({"type":"thinking","thinking":"","signature":""}),
                    BlockKind::Tool { id, name } => {
                        json!({"type":"tool_use","id":id,"name":name,"input":{}})
                    }
                };
                vec![sse_json(
                    Some("content_block_start"),
                    &json!({"type":"content_block_start","index":index,"content_block":content_block}),
                )]
            }
            PivotEvent::TextDelta { index, text } => vec![sse_json(
                Some("content_block_delta"),
                &json!({"type":"content_block_delta","index":index,"delta":{"type":"text_delta","text":text}}),
            )],
            PivotEvent::ReasoningDelta { index, text } => vec![sse_json(
                Some("content_block_delta"),
                &json!({"type":"content_block_delta","index":index,"delta":{"type":"thinking_delta","thinking":text}}),
            )],
            PivotEvent::SignatureDelta { index, signature } => vec![sse_json(
                Some("content_block_delta"),
                &json!({"type":"content_block_delta","index":index,"delta":{"type":"signature_delta","signature":signature}}),
            )],
            PivotEvent::ArgumentsDelta { index, arguments } => vec![sse_json(
                Some("content_block_delta"),
                &json!({"type":"content_block_delta","index":index,"delta":{"type":"input_json_delta","partial_json":arguments}}),
            )],
            PivotEvent::BlockStop { index } => vec![sse_json(
                Some("content_block_stop"),
                &json!({"type":"content_block_stop","index":index}),
            )],
            PivotEvent::MessageDelta { stop_reason, usage } => vec![sse_json(
                Some("message_delta"),
                &json!({"type":"message_delta","delta":{"stop_reason":stop_reason,"stop_sequence":null},"usage":anthropic_usage_json(&usage)}),
            )],
            PivotEvent::Stop => vec![sse_json(
                Some("message_stop"),
                &json!({"type":"message_stop"}),
            )],
            PivotEvent::Error { kind, message } => vec![sse_json(
                Some("error"),
                &json!({"type":"error","error":{"type":kind,"message":message}}),
            )],
        }
    }

    fn encode_chat(&mut self, event: PivotEvent) -> Vec<Bytes> {
        match event {
            PivotEvent::Start { id, model, usage } => {
                self.output.id = chat_id(&id);
                self.output.model = model;
                self.output.usage.merge(usage);
                vec![self.chat_chunk(json!({"role":"assistant"}), Value::Null)]
            }
            PivotEvent::BlockStart { index, kind } => {
                let tool_index = matches!(kind, BlockKind::Tool { .. }).then(|| {
                    let value = self.output.next_tool_index;
                    self.output.next_tool_index += 1;
                    value
                });
                let mut frames = Vec::new();
                if let BlockKind::Tool { ref id, ref name } = kind {
                    frames.push(self.chat_chunk(
                        json!({"tool_calls":[{"index":tool_index.unwrap_or(0),"id":id,"type":"function","function":{"name":name,"arguments":""}}]}),
                        Value::Null,
                    ));
                }
                self.output.blocks.insert(
                    index,
                    OutputBlock {
                        kind,
                        content: String::new(),
                        tool_index,
                        output_index: None,
                        custom_input_emitted: 0,
                        closed: false,
                    },
                );
                frames
            }
            PivotEvent::TextDelta { index, text } => {
                if let Some(block) = self.output.blocks.get_mut(&index) {
                    block.content.push_str(&text);
                }
                vec![self.chat_chunk(json!({"content":text}), Value::Null)]
            }
            PivotEvent::ReasoningDelta { index, text } => {
                if let Some(block) = self.output.blocks.get_mut(&index) {
                    block.content.push_str(&text);
                }
                vec![self.chat_chunk(json!({"reasoning_content":text}), Value::Null)]
            }
            PivotEvent::SignatureDelta { .. } => Vec::new(),
            PivotEvent::ArgumentsDelta { index, arguments } => {
                let Some(block) = self.output.blocks.get_mut(&index) else {
                    return Vec::new();
                };
                block.content.push_str(&arguments);
                let tool_index = block.tool_index.unwrap_or(0);
                vec![self.chat_chunk(
                    json!({"tool_calls":[{"index":tool_index,"function":{"arguments":arguments}}]}),
                    Value::Null,
                )]
            }
            PivotEvent::BlockStop { index } => {
                if let Some(block) = self.output.blocks.get_mut(&index) {
                    block.closed = true;
                }
                Vec::new()
            }
            PivotEvent::MessageDelta { stop_reason, usage } => {
                self.output.stop_reason = Some(stop_reason);
                self.output.usage.merge(usage);
                Vec::new()
            }
            PivotEvent::Stop => {
                let mut frames = Vec::new();
                if !self.output.finish_emitted {
                    let reason = self
                        .output
                        .stop_reason
                        .clone()
                        .unwrap_or_else(|| "end_turn".to_string());
                    frames.extend(self.emit_chat_finish(&reason));
                }
                if !self.output.terminal {
                    self.output.terminal = true;
                    frames.push(done_frame());
                }
                frames
            }
            PivotEvent::Error { kind, message } => {
                self.output.terminal = true;
                vec![
                    sse_json(None, &json!({"error":{"type":kind,"message":message}})),
                    done_frame(),
                ]
            }
        }
    }

    fn chat_chunk(&self, delta: Value, finish_reason: Value) -> Bytes {
        sse_json(
            None,
            &json!({
                "id":if self.output.id.is_empty() { "chatcmpl-ocg" } else { &self.output.id },
                "object":"chat.completion.chunk","created":unix_seconds(),
                "model":if self.output.model.is_empty() { &self.model } else { &self.output.model },
                "choices":[{"index":0,"delta":delta,"finish_reason":finish_reason}]
            }),
        )
    }

    fn emit_chat_finish(&mut self, stop_reason: &str) -> Vec<Bytes> {
        if self.output.finish_emitted {
            return Vec::new();
        }
        self.output.finish_emitted = true;
        let mut frames =
            vec![self.chat_chunk(json!({}), json!(anthropic_stop_to_chat(stop_reason)))];
        if self.output.usage.seen {
            frames.push(sse_json(
                None,
                &json!({
                    "id":self.output.id,"object":"chat.completion.chunk","created":unix_seconds(),
                    "model":self.output.model,"choices":[],"usage":chat_usage_json(&self.output.usage)
                }),
            ));
        }
        frames
    }

    fn encode_gemini(&mut self, event: PivotEvent) -> Result<Vec<Bytes>, ProtocolError> {
        match event {
            PivotEvent::Start { id, model, usage } => {
                self.output.id = id;
                self.output.model = model;
                self.output.usage.merge(usage);
                Ok(Vec::new())
            }
            PivotEvent::BlockStart { index, kind } => {
                self.output.blocks.insert(
                    index,
                    OutputBlock {
                        kind,
                        content: String::new(),
                        tool_index: None,
                        output_index: None,
                        custom_input_emitted: 0,
                        closed: false,
                    },
                );
                Ok(Vec::new())
            }
            PivotEvent::TextDelta { index, text } => {
                if let Some(block) = self.output.blocks.get_mut(&index) {
                    block.content.push_str(&text);
                }
                if text.is_empty() {
                    Ok(Vec::new())
                } else {
                    Ok(vec![self.gemini_chunk(
                        vec![json!({ "text": text })],
                        None,
                        false,
                    )])
                }
            }
            PivotEvent::ReasoningDelta { index, text } => {
                if let Some(block) = self.output.blocks.get_mut(&index) {
                    block.content.push_str(&text);
                }
                Ok(Vec::new())
            }
            PivotEvent::SignatureDelta { .. } => Ok(Vec::new()),
            PivotEvent::ArgumentsDelta { index, arguments } => {
                if let Some(block) = self.output.blocks.get_mut(&index) {
                    block.content.push_str(&arguments);
                }
                Ok(Vec::new())
            }
            PivotEvent::BlockStop { index } => {
                if let Some(block) = self.output.blocks.get_mut(&index) {
                    block.closed = true;
                }
                Ok(Vec::new())
            }
            PivotEvent::MessageDelta { stop_reason, usage } => {
                self.output.stop_reason = Some(stop_reason);
                self.output.usage.merge(usage);
                Ok(Vec::new())
            }
            PivotEvent::Stop => {
                if self.output.terminal {
                    return Ok(Vec::new());
                }
                let mut has_text = false;
                let mut tool_parts = Vec::new();
                for block in self.output.blocks.values() {
                    match &block.kind {
                        BlockKind::Text => has_text |= !block.content.is_empty(),
                        BlockKind::Tool { id, name } => {
                            let args = if block.content.trim().is_empty() {
                                json!({})
                            } else {
                                serde_json::from_str::<Value>(&block.content).map_err(|error| {
                                    ProtocolError::new(format!(
                                        "upstream tool arguments are invalid JSON: {error}"
                                    ))
                                })?
                            };
                            if !args.is_object() {
                                return Err(ProtocolError::new(
                                    "upstream tool arguments must be a JSON object",
                                ));
                            }
                            tool_parts.push(json!({
                                "functionCall": { "id": id, "name": name, "args": args },
                                "thoughtSignature": "skip_thought_signature_validator"
                            }));
                        }
                        BlockKind::Reasoning => {}
                    }
                }
                let stop_reason = self.output.stop_reason.as_deref().unwrap_or("end_turn");
                let finish_reason = match stop_reason {
                    "max_tokens" | "model_context_window_exceeded" => "MAX_TOKENS",
                    "refusal" => "SAFETY",
                    "end_turn" | "stop_sequence" | "tool_use" => "STOP",
                    _ => "OTHER",
                };
                if !has_text && tool_parts.is_empty() && finish_reason == "STOP" {
                    return Err(ProtocolError::new(
                        "upstream stream ended without text or a function call",
                    ));
                }
                self.output.terminal = true;
                Ok(vec![self.gemini_chunk(
                    tool_parts,
                    Some(finish_reason),
                    true,
                )])
            }
            PivotEvent::Error { message, .. } => {
                self.output.terminal = true;
                Ok(vec![sse_json(
                    None,
                    &json!({
                        "error":{"code":500,"message":message,"status":"INTERNAL"}
                    }),
                )])
            }
        }
    }

    fn gemini_chunk(
        &self,
        parts: Vec<Value>,
        finish_reason: Option<&str>,
        include_usage: bool,
    ) -> Bytes {
        let mut candidate = json!({ "index": 0 });
        if !parts.is_empty() {
            candidate["content"] = json!({ "role": "model", "parts": parts });
        }
        if let Some(reason) = finish_reason {
            candidate["finishReason"] = json!(reason);
        }
        let mut response = json!({
            "candidates": [candidate],
            "modelVersion": if self.output.model.is_empty() { &self.model } else { &self.output.model },
            "responseId": if self.output.id.is_empty() { "ocg_response" } else { &self.output.id }
        });
        if include_usage && self.output.usage.seen {
            response["usageMetadata"] = gemini_usage_json(&self.output.usage);
        }
        sse_json(None, &response)
    }

    fn encode_responses(&mut self, event: PivotEvent) -> Vec<Bytes> {
        match event {
            PivotEvent::Start { id, model, usage } => {
                self.output.id = responses_id(&id);
                self.output.model = model;
                self.output.created_at = unix_seconds();
                self.output.usage.merge(usage);
                let response = self.response_object("in_progress", Value::Null, Vec::new());
                vec![self.responses_event("response.created", json!({"response":response}))]
            }
            PivotEvent::BlockStart { index, kind } => {
                let output_index = self.output.next_output_index;
                self.output.next_output_index += 1;
                let mut frames = Vec::new();
                let item = match &kind {
                    BlockKind::Text => {
                        json!({"type":"message","id":format!("msg_{output_index}"),"status":"in_progress","role":"assistant","content":[]})
                    }
                    BlockKind::Reasoning => {
                        json!({"type":"reasoning","id":format!("rs_{output_index}"),"summary":[]})
                    }
                    BlockKind::Tool { id, name } => {
                        let (response_name, namespace, custom) = self.response_tool_identity(name);
                        let mut item = if custom {
                            json!({"type":"custom_tool_call","id":format!("ctc_{output_index}"),"call_id":id,"name":response_name,"input":"","status":"in_progress"})
                        } else {
                            json!({"type":"function_call","id":format!("fc_{output_index}"),"call_id":id,"name":response_name,"arguments":"","status":"in_progress"})
                        };
                        if let Some(namespace) = namespace {
                            item["namespace"] = json!(namespace);
                        }
                        item
                    }
                };
                frames.push(self.responses_event(
                    "response.output_item.added",
                    json!({"output_index":output_index,"item":item}),
                ));
                match kind {
                    BlockKind::Text => frames.push(self.responses_event(
                        "response.content_part.added",
                        json!({"item_id":format!("msg_{output_index}"),"output_index":output_index,"content_index":0,"part":{"type":"output_text","text":"","annotations":[],"logprobs":[]}}),
                    )),
                    BlockKind::Reasoning => frames.push(self.responses_event(
                        "response.reasoning_summary_part.added",
                        json!({"item_id":format!("rs_{output_index}"),"output_index":output_index,"summary_index":0,"part":{"type":"summary_text","text":""}}),
                    )),
                    BlockKind::Tool { .. } => {}
                }
                self.output.blocks.insert(
                    index,
                    OutputBlock {
                        kind,
                        content: String::new(),
                        tool_index: None,
                        output_index: Some(output_index),
                        custom_input_emitted: 0,
                        closed: false,
                    },
                );
                frames
            }
            PivotEvent::TextDelta { index, text } => {
                let Some(block) = self.output.blocks.get_mut(&index) else {
                    return Vec::new();
                };
                block.content.push_str(&text);
                let output_index = block.output_index.unwrap_or(0);
                vec![self.responses_event(
                    "response.output_text.delta",
                    json!({"item_id":format!("msg_{output_index}"),"output_index":output_index,"content_index":0,"delta":text,"logprobs":[]}),
                )]
            }
            PivotEvent::ReasoningDelta { index, text } => {
                let Some(block) = self.output.blocks.get_mut(&index) else {
                    return Vec::new();
                };
                block.content.push_str(&text);
                let output_index = block.output_index.unwrap_or(0);
                vec![self.responses_event(
                    "response.reasoning_summary_text.delta",
                    json!({"item_id":format!("rs_{output_index}"),"output_index":output_index,"summary_index":0,"delta":text}),
                )]
            }
            PivotEvent::SignatureDelta { .. } => Vec::new(),
            PivotEvent::ArgumentsDelta { index, arguments } => {
                let Some(block) = self.output.blocks.get_mut(&index) else {
                    return Vec::new();
                };
                block.content.push_str(&arguments);
                let output_index = block.output_index.unwrap_or(0);
                if matches!(
                    &block.kind,
                    BlockKind::Tool { name, .. } if self.custom_tools.contains(name)
                ) {
                    let Some(input) = custom_tool_input_prefix(&block.content) else {
                        return Vec::new();
                    };
                    let Some(delta) = input.get(block.custom_input_emitted..) else {
                        return Vec::new();
                    };
                    let delta = delta.to_string();
                    block.custom_input_emitted = input.len();
                    if delta.is_empty() {
                        return Vec::new();
                    }
                    return vec![self.responses_event(
                        "response.custom_tool_call_input.delta",
                        json!({"item_id":format!("ctc_{output_index}"),"output_index":output_index,"delta":delta}),
                    )];
                }
                vec![self.responses_event(
                    "response.function_call_arguments.delta",
                    json!({"item_id":format!("fc_{output_index}"),"output_index":output_index,"delta":arguments}),
                )]
            }
            PivotEvent::BlockStop { index } => self.close_response_block(index),
            PivotEvent::MessageDelta { stop_reason, usage } => {
                self.output.stop_reason = Some(stop_reason);
                self.output.usage.merge(usage);
                Vec::new()
            }
            PivotEvent::Stop => self.emit_response_completed(),
            PivotEvent::Error { kind, message } => {
                self.output.terminal = true;
                let response = self.failed_response_object(&kind, &message);
                vec![self.responses_event("response.failed", json!({"response":response}))]
            }
        }
    }

    fn close_response_block(&mut self, index: usize) -> Vec<Bytes> {
        let Some(block) = self.output.blocks.get_mut(&index) else {
            return Vec::new();
        };
        if block.closed {
            return Vec::new();
        }
        block.closed = true;
        let output_index = block.output_index.unwrap_or(0);
        let content = block.content.clone();
        let kind = block.kind.clone();
        match kind {
            BlockKind::Text => vec![
                self.responses_event("response.output_text.done", json!({"item_id":format!("msg_{output_index}"),"output_index":output_index,"content_index":0,"text":content,"logprobs":[]})),
                self.responses_event("response.content_part.done", json!({"item_id":format!("msg_{output_index}"),"output_index":output_index,"content_index":0,"part":{"type":"output_text","text":content,"annotations":[],"logprobs":[]}})),
                self.responses_event("response.output_item.done", json!({"output_index":output_index,"item":{"type":"message","id":format!("msg_{output_index}"),"status":"completed","role":"assistant","content":[{"type":"output_text","text":content,"annotations":[],"logprobs":[]}]}})),
            ],
            BlockKind::Reasoning => {
                let item = self.response_reasoning_item(index, output_index, &content);
                vec![
                    self.responses_event("response.reasoning_summary_text.done", json!({"item_id":format!("rs_{output_index}"),"output_index":output_index,"summary_index":0,"text":content})),
                    self.responses_event("response.reasoning_summary_part.done", json!({"item_id":format!("rs_{output_index}"),"output_index":output_index,"summary_index":0,"part":{"type":"summary_text","text":content}})),
                    self.responses_event("response.output_item.done", json!({"output_index":output_index,"item":item})),
                ]
            }
            BlockKind::Tool { id, name } => {
                let (response_name, namespace, custom) = self.response_tool_identity(&name);
                if custom {
                    let input = custom_tool_input(&content);
                    let mut item = json!({"type":"custom_tool_call","id":format!("ctc_{output_index}"),"call_id":id,"name":response_name,"input":input,"status":"completed"});
                    if let Some(namespace) = namespace {
                        item["namespace"] = json!(namespace);
                    }
                    vec![
                        self.responses_event("response.custom_tool_call_input.done", json!({"item_id":format!("ctc_{output_index}"),"output_index":output_index,"input":input})),
                        self.responses_event("response.output_item.done", json!({"output_index":output_index,"item":item})),
                    ]
                } else {
                    let mut item = json!({"type":"function_call","id":format!("fc_{output_index}"),"call_id":id,"name":response_name,"arguments":content,"status":"completed"});
                    if let Some(namespace) = namespace {
                        item["namespace"] = json!(namespace);
                    }
                    vec![
                        self.responses_event("response.function_call_arguments.done", json!({"item_id":format!("fc_{output_index}"),"output_index":output_index,"name":response_name,"arguments":content})),
                        self.responses_event("response.output_item.done", json!({"output_index":output_index,"item":item})),
                    ]
                }
            }
        }
    }

    fn emit_response_completed(&mut self) -> Vec<Bytes> {
        if self.output.terminal {
            return Vec::new();
        }
        let open: Vec<usize> = self
            .output
            .blocks
            .iter()
            .filter_map(|(index, block)| (!block.closed).then_some(*index))
            .collect();
        let mut frames = Vec::new();
        for index in open {
            frames.extend(self.close_response_block(index));
        }
        self.output.terminal = true;
        let (status, details) = match self.output.stop_reason.as_deref() {
            Some("max_tokens" | "model_context_window_exceeded") => {
                ("incomplete", json!({"reason":"max_output_tokens"}))
            }
            Some("refusal") => ("incomplete", json!({"reason":"content_filter"})),
            _ => ("completed", Value::Null),
        };
        let output = self.response_output_items();
        let response = self.response_object(status, details, output);
        frames.push(self.responses_event(
            if status == "incomplete" {
                "response.incomplete"
            } else {
                "response.completed"
            },
            json!({"response":response}),
        ));
        frames
    }

    fn response_output_items(&self) -> Vec<Value> {
        let mut blocks: Vec<(&usize, &OutputBlock)> = self.output.blocks.iter().collect();
        blocks.sort_by_key(|(_, block)| block.output_index.unwrap_or(u64::MAX));
        blocks
            .into_iter()
            .map(|(source_index, block)| {
                let output_index = block.output_index.unwrap_or(0);
                match &block.kind {
                    BlockKind::Text => json!({"type":"message","id":format!("msg_{output_index}"),"status":"completed","role":"assistant","content":[{"type":"output_text","text":block.content,"annotations":[],"logprobs":[]}]}),
                    BlockKind::Reasoning => self.response_reasoning_item(*source_index, output_index, &block.content),
                    BlockKind::Tool { id, name } => {
                        let (response_name, namespace, custom) =
                            self.response_tool_identity(name);
                        let mut item = if custom {
                            json!({"type":"custom_tool_call","id":format!("ctc_{output_index}"),"call_id":id,"name":response_name,"input":custom_tool_input(&block.content),"status":"completed"})
                        } else {
                            json!({"type":"function_call","id":format!("fc_{output_index}"),"call_id":id,"name":response_name,"arguments":block.content,"status":"completed"})
                        };
                        if let Some(namespace) = namespace {
                            item["namespace"] = json!(namespace);
                        }
                        item
                    }
                }
            })
            .collect()
    }

    fn response_tool_identity(&self, upstream_name: &str) -> (String, Option<String>, bool) {
        if let Some(mapping) = self.namespace_tools.get(upstream_name) {
            return (
                mapping.name.clone(),
                Some(mapping.namespace.clone()),
                mapping.custom,
            );
        }
        (
            upstream_name.to_string(),
            None,
            self.custom_tools.contains(upstream_name),
        )
    }

    fn response_reasoning_item(
        &self,
        source_index: usize,
        output_index: u64,
        content: &str,
    ) -> Value {
        let summary = if content.is_empty() {
            Vec::new()
        } else {
            vec![json!({"type":"summary_text","text":content})]
        };
        let mut item = json!({
            "type":"reasoning",
            "id":format!("rs_{output_index}"),
            "summary":summary
        });
        let encrypted_content = self
            .input
            .anthropic_reasoning
            .get(&source_index)
            .and_then(encode_anthropic_thinking_block)
            .or_else(|| {
                (self.source == ApiFormat::ChatCompletions)
                    .then(|| encode_chat_reasoning(content))
                    .flatten()
            });
        if let Some(encrypted_content) = encrypted_content {
            item["encrypted_content"] = json!(encrypted_content);
        }
        item
    }

    fn response_object(
        &self,
        status: &str,
        incomplete_details: Value,
        output: Vec<Value>,
    ) -> Value {
        let created_at = if self.output.created_at == 0 {
            unix_seconds()
        } else {
            self.output.created_at
        };
        json!({
            "id":if self.output.id.is_empty() { "resp_ocg" } else { &self.output.id },
            "object":"response","created_at":created_at,"status":status,"background":false,
            "completed_at":if status == "completed" { json!(unix_seconds()) } else { Value::Null },"error":null,
            "incomplete_details":incomplete_details,"instructions":null,"max_output_tokens":null,
            "max_tool_calls":null,
            "model":if self.output.model.is_empty() { &self.model } else { &self.output.model },
            "output":output,"parallel_tool_calls":self.response_parallel_tool_calls,"previous_response_id":null,
            "reasoning":{"effort":null,"summary":null},"store":false,"temperature":null,
            "text":{"format":{"type":"text"}},"tool_choice":self.response_tool_choice,"tools":self.response_tools,"top_p":null,
            "truncation":"disabled","usage":if self.output.usage.seen { responses_usage_json(&self.output.usage) } else { Value::Null },
            "user":null,"metadata":{}
        })
    }

    fn failed_response_object(&self, code: &str, message: &str) -> Value {
        let mut response = self.response_object("failed", Value::Null, Vec::new());
        response["error"] = json!({"code":code,"message":message});
        response
    }

    fn responses_event(&mut self, event_type: &str, fields: Value) -> Bytes {
        let mut object = match fields {
            Value::Object(map) => map,
            _ => Map::new(),
        };
        object.insert("type".to_string(), json!(event_type));
        object.insert("sequence_number".to_string(), json!(self.output.sequence));
        self.output.sequence += 1;
        sse_json(Some(event_type), &Value::Object(object))
    }
}

impl Usage {
    fn merge(&mut self, other: Usage) {
        if !other.seen {
            return;
        }
        self.seen = true;
        self.input = self.input.max(other.input);
        self.output = self.output.max(other.output);
        self.cached = self.cached.max(other.cached);
        self.cache_creation = self.cache_creation.max(other.cache_creation);
    }
}

fn drain_frames(buffer: &mut BytesMut) -> Vec<Bytes> {
    let mut frames = Vec::new();
    while let Some((index, delimiter_len)) = find_boundary(buffer) {
        let frame = buffer.split_to(index + delimiter_len);
        frames.push(frame.freeze());
    }
    frames
}

fn find_boundary(bytes: &[u8]) -> Option<(usize, usize)> {
    let lf = bytes
        .windows(2)
        .position(|window| window == b"\n\n")
        .map(|i| (i, 2));
    let crlf = bytes
        .windows(4)
        .position(|window| window == b"\r\n\r\n")
        .map(|i| (i, 4));
    match (lf, crlf) {
        (Some(a), Some(b)) => Some(if a.0 <= b.0 { a } else { b }),
        (Some(a), None) => Some(a),
        (None, Some(b)) => Some(b),
        (None, None) => None,
    }
}

fn parse_sse_frame(frame: &[u8]) -> Result<Option<(Option<String>, String)>, ProtocolError> {
    let text = std::str::from_utf8(frame)
        .map_err(|e| ProtocolError::new(format!("invalid UTF-8 in SSE event: {e}")))?;
    let mut event = None;
    let mut data = Vec::new();
    for line in text.lines() {
        let line = line.trim_end_matches('\r');
        if let Some(value) = line.strip_prefix("event:") {
            event = Some(value.trim_start().to_string());
        } else if let Some(value) = line.strip_prefix("data:") {
            data.push(value.strip_prefix(' ').unwrap_or(value));
        } else if line == "data" {
            data.push("");
        }
    }
    if data.is_empty() {
        Ok(None)
    } else {
        Ok(Some((event, data.join("\n"))))
    }
}

fn sse_json(event: Option<&str>, value: &Value) -> Bytes {
    let prefix = event.map_or_else(String::new, |name| format!("event: {name}\n"));
    Bytes::from(format!("{prefix}data: {value}\n\n"))
}

fn done_frame() -> Bytes {
    Bytes::from_static(b"data: [DONE]\n\n")
}

fn append_json_string(value: Option<&mut Value>, key: &str, suffix: &str) {
    let Some(object) = value.and_then(Value::as_object_mut) else {
        return;
    };
    let mut combined = object
        .get(key)
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    combined.push_str(suffix);
    object.insert(key.to_string(), json!(combined));
}

fn custom_tool_input(arguments: &str) -> String {
    serde_json::from_str::<Value>(arguments)
        .ok()
        .and_then(|value| {
            value
                .get("input")
                .and_then(Value::as_str)
                .map(str::to_string)
                .or_else(|| value.as_str().map(str::to_string))
        })
        .unwrap_or_else(|| arguments.to_string())
}

fn custom_tool_input_prefix(arguments: &str) -> Option<String> {
    if let Ok(value) = serde_json::from_str::<Value>(arguments) {
        return value
            .get("input")
            .and_then(Value::as_str)
            .or_else(|| value.as_str())
            .map(str::to_string);
    }
    let encoded = if let Some(offset) = arguments.find("\"input\"") {
        let after_key = &arguments[offset + "\"input\"".len()..];
        after_key
            .split_once(':')?
            .1
            .trim_start()
            .strip_prefix('"')?
    } else {
        arguments.trim_start().strip_prefix('"')?
    };
    let mut escaped = false;
    let mut end = encoded.len();
    for (index, character) in encoded.char_indices() {
        if escaped {
            escaped = false;
        } else if character == '\\' {
            escaped = true;
        } else if character == '"' {
            end = index;
            break;
        }
    }
    serde_json::from_str(&format!("\"{}\"", &encoded[..end])).ok()
}

fn string_at(value: &Value, pointer: &str, fallback: &str) -> String {
    value
        .pointer(pointer)
        .and_then(Value::as_str)
        .unwrap_or(fallback)
        .to_string()
}

fn u64_at(value: &Value, pointer: &str) -> Option<u64> {
    value.pointer(pointer).and_then(Value::as_u64)
}

fn anthropic_usage(value: Option<&Value>) -> Usage {
    let Some(value) = value.filter(|v| v.is_object()) else {
        return Usage::default();
    };
    let cached = value
        .get("cache_read_input_tokens")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let cache_creation = value
        .get("cache_creation_input_tokens")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    Usage {
        seen: true,
        input: value
            .get("input_tokens")
            .and_then(Value::as_u64)
            .unwrap_or(0)
            .saturating_add(cached)
            .saturating_add(cache_creation),
        output: value
            .get("output_tokens")
            .and_then(Value::as_u64)
            .unwrap_or(0),
        cached,
        cache_creation,
    }
}

fn chat_usage(value: Option<&Value>) -> Usage {
    let Some(value) = value.filter(|v| v.is_object()) else {
        return Usage::default();
    };
    Usage {
        seen: true,
        input: value
            .get("prompt_tokens")
            .and_then(Value::as_u64)
            .unwrap_or(0),
        output: value
            .get("completion_tokens")
            .and_then(Value::as_u64)
            .unwrap_or(0),
        cached: value
            .pointer("/prompt_tokens_details/cached_tokens")
            .and_then(Value::as_u64)
            .unwrap_or(0),
        cache_creation: 0,
    }
}

fn responses_usage(value: Option<&Value>) -> Usage {
    let Some(value) = value.filter(|v| v.is_object()) else {
        return Usage::default();
    };
    Usage {
        seen: true,
        input: value
            .get("input_tokens")
            .and_then(Value::as_u64)
            .unwrap_or(0),
        output: value
            .get("output_tokens")
            .and_then(Value::as_u64)
            .unwrap_or(0),
        cached: value
            .pointer("/input_tokens_details/cached_tokens")
            .and_then(Value::as_u64)
            .unwrap_or(0),
        cache_creation: 0,
    }
}

fn anthropic_usage_json(usage: &Usage) -> Value {
    let uncached = usage
        .input
        .saturating_sub(usage.cached.saturating_add(usage.cache_creation));
    json!({
        "input_tokens":uncached,"output_tokens":usage.output,
        "cache_read_input_tokens":usage.cached,"cache_creation_input_tokens":usage.cache_creation
    })
}

fn chat_usage_json(usage: &Usage) -> Value {
    json!({
        "prompt_tokens":usage.input,"completion_tokens":usage.output,
        "total_tokens":usage.input + usage.output,
        "prompt_tokens_details":{"cached_tokens":usage.cached}
    })
}

fn responses_usage_json(usage: &Usage) -> Value {
    json!({
        "input_tokens":usage.input,"output_tokens":usage.output,
        "total_tokens":usage.input + usage.output,
        "input_tokens_details":{"cached_tokens":usage.cached},
        "output_tokens_details":{"reasoning_tokens":0}
    })
}

fn gemini_usage_json(usage: &Usage) -> Value {
    json!({
        "promptTokenCount": usage.input,
        "candidatesTokenCount": usage.output,
        "totalTokenCount": usage.input.saturating_add(usage.output),
        "cachedContentTokenCount": usage.cached,
        "thoughtsTokenCount": 0
    })
}

fn chat_stop_to_anthropic(reason: &str) -> &'static str {
    match reason {
        "length" => "max_tokens",
        "tool_calls" | "function_call" => "tool_use",
        "content_filter" => "refusal",
        _ => "end_turn",
    }
}

fn anthropic_stop_to_chat(reason: &str) -> &'static str {
    match reason {
        "max_tokens" => "length",
        "tool_use" => "tool_calls",
        "refusal" => "content_filter",
        _ => "stop",
    }
}

fn anthropic_id(id: &str) -> String {
    if id.starts_with("msg_") {
        id.to_string()
    } else {
        format!("msg_{id}")
    }
}

fn chat_id(id: &str) -> String {
    if id.starts_with("chatcmpl-") {
        id.to_string()
    } else {
        format!("chatcmpl-{id}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn plan(client: ApiFormat, upstream: ApiFormat) -> RequestPlan {
        RequestPlan {
            client,
            upstream,
            model: "test-model".to_string(),
            stream: true,
            body: Bytes::new(),
            custom_tools: Vec::new(),
            namespace_tools: Vec::new(),
            response_parallel_tool_calls: true,
            response_tool_choice: json!("auto"),
            response_tools: Vec::new(),
        }
    }

    fn convert(client: ApiFormat, upstream: ApiFormat, source: &str) -> String {
        let mut converter = StreamConverter::new(&plan(client, upstream));
        let bytes = source.as_bytes();
        let split = source.find('好').unwrap_or(bytes.len() / 2) + 1;
        let mut output = converter
            .process_chunk(Bytes::copy_from_slice(&bytes[..split]))
            .expect("first split should parse");
        output.extend(
            converter
                .process_chunk(Bytes::copy_from_slice(&bytes[split..]))
                .expect("second split should parse"),
        );
        output.extend(converter.finish().expect("stream should finish"));
        String::from_utf8(output.concat()).expect("output must be UTF-8")
    }

    #[test]
    fn same_protocol_is_byte_passthrough() {
        let mut converter = StreamConverter::new(&plan(ApiFormat::Messages, ApiFormat::Messages));
        let chunk = Bytes::from_static(
            b"event: message_start\ndata: {\"type\":\"message_start\",\"message\":{\"id\":\"m\",\"model\":\"m\"}}\n\nevent: message_stop\ndata: {\"type\":\"message_stop\"}\n\n",
        );
        assert_eq!(
            converter.process_chunk(chunk.clone()).unwrap().concat(),
            chunk.as_ref()
        );
        assert!(converter.finish().unwrap().is_empty());
    }

    #[test]
    fn same_protocol_drops_events_after_terminal() {
        let mut converter = StreamConverter::new(&plan(ApiFormat::Messages, ApiFormat::Messages));
        let chunk = Bytes::from_static(
            b"event: message_start\ndata: {\"type\":\"message_start\",\"message\":{\"id\":\"m\",\"model\":\"m\"}}\n\nevent: message_stop\ndata: {\"type\":\"message_stop\"}\n\nevent: error\ndata: {\"type\":\"error\",\"error\":{\"message\":\"late\"}}\n\n",
        );
        let output = converter.process_chunk(chunk).unwrap().concat();
        let output = String::from_utf8(output).unwrap();
        assert!(output.contains("message_stop"));
        assert!(!output.contains("late"));

        let later = Bytes::from_static(
            b"event: error\ndata: {\"type\":\"error\",\"error\":{\"message\":\"later\"}}\n\n",
        );
        assert!(converter.process_chunk(later).unwrap().is_empty());
        assert!(converter.finish().unwrap().is_empty());
    }

    #[test]
    fn empty_data_heartbeat_is_ignored() {
        let mut converter =
            StreamConverter::new(&plan(ApiFormat::Messages, ApiFormat::ChatCompletions));
        assert!(
            converter
                .process_chunk(Bytes::from_static(b"data:\n\n"))
                .unwrap()
                .is_empty()
        );
    }

    #[test]
    fn chat_to_gemini_streams_text_usage_and_finishes_without_done_sentinel() {
        let source = concat!(
            "data: {\"id\":\"resp_1\",\"model\":\"deepseek-v4-flash\",\"choices\":[{\"index\":0,\"delta\":{\"content\":\"Hel\"},\"finish_reason\":null}]}\n\n",
            "data: {\"id\":\"resp_1\",\"model\":\"deepseek-v4-flash\",\"choices\":[{\"index\":0,\"delta\":{\"content\":\"lo\"},\"finish_reason\":\"stop\"}],\"usage\":{\"prompt_tokens\":7,\"completion_tokens\":2,\"prompt_tokens_details\":{\"cached_tokens\":1}}}\n\n",
            "data: [DONE]\n\n"
        );
        let output = convert(ApiFormat::Gemini, ApiFormat::ChatCompletions, source);
        assert!(output.contains("\"text\":\"Hel\""));
        assert!(output.contains("\"text\":\"lo\""));
        assert!(output.contains("\"finishReason\":\"STOP\""));
        assert!(output.contains("\"promptTokenCount\":7"));
        assert!(output.contains("\"candidatesTokenCount\":2"));
        assert!(!output.contains("[DONE]"));
        assert_eq!(output.matches("\"responseId\":\"resp_1\"").count(), 3);
    }

    #[test]
    fn messages_to_gemini_buffers_parallel_function_calls_until_valid_json() {
        let source = concat!(
            "event: message_start\ndata: {\"type\":\"message_start\",\"message\":{\"id\":\"msg_2\",\"model\":\"minimax-m3\",\"usage\":{\"input_tokens\":12}}}\n\n",
            "event: content_block_start\ndata: {\"type\":\"content_block_start\",\"index\":0,\"content_block\":{\"type\":\"tool_use\",\"id\":\"call_a\",\"name\":\"read_file\",\"input\":{}}}\n\n",
            "event: content_block_delta\ndata: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"input_json_delta\",\"partial_json\":\"{\\\"path\\\":\\\"Cargo.toml\\\"}\"}}\n\n",
            "event: content_block_stop\ndata: {\"type\":\"content_block_stop\",\"index\":0}\n\n",
            "event: content_block_start\ndata: {\"type\":\"content_block_start\",\"index\":1,\"content_block\":{\"type\":\"tool_use\",\"id\":\"call_b\",\"name\":\"list_dir\",\"input\":{}}}\n\n",
            "event: content_block_delta\ndata: {\"type\":\"content_block_delta\",\"index\":1,\"delta\":{\"type\":\"input_json_delta\",\"partial_json\":\"{}\"}}\n\n",
            "event: content_block_stop\ndata: {\"type\":\"content_block_stop\",\"index\":1}\n\n",
            "event: message_delta\ndata: {\"type\":\"message_delta\",\"delta\":{\"stop_reason\":\"tool_use\"},\"usage\":{\"output_tokens\":3}}\n\n",
            "event: message_stop\ndata: {\"type\":\"message_stop\"}\n\n"
        );
        let output = convert(ApiFormat::Gemini, ApiFormat::Messages, source);
        assert_eq!(output.matches("\"functionCall\"").count(), 2);
        assert!(output.contains("\"id\":\"call_a\""));
        assert!(output.contains("\"path\":\"Cargo.toml\""));
        assert!(output.contains("skip_thought_signature_validator"));
        assert!(output.contains("\"finishReason\":\"STOP\""));
        assert!(output.contains("\"promptTokenCount\":12"));
        assert!(!output.contains("[DONE]"));
    }

    #[test]
    fn gemini_stream_errors_use_google_envelope_without_done() {
        let converter = StreamConverter::new(&plan(ApiFormat::Gemini, ApiFormat::Messages));
        let output = String::from_utf8(converter.error_event("boom").concat()).unwrap();
        assert!(output.contains("\"code\":500"));
        assert!(output.contains("\"status\":\"INTERNAL\""));
        assert!(output.contains("\"message\":\"boom\""));
        assert!(!output.contains("[DONE]"));
    }

    #[test]
    fn chat_to_messages_handles_utf8_reasoning_parallel_tools_and_usage() {
        let source = concat!(
            "data: {\"id\":\"c1\",\"model\":\"m\",\"choices\":[{\"index\":0,\"delta\":{\"reasoning_content\":\"想好\",\"content\":\"你好\",\"tool_calls\":[{\"index\":0,\"id\":\"a\",\"function\":{\"name\":\"one\",\"arguments\":\"{\\\"x\\\":\"}},{\"index\":1,\"id\":\"b\",\"function\":{\"name\":\"two\",\"arguments\":\"{\\\"y\\\":\"}}]},\"finish_reason\":null}]}\n\n",
            "data: {\"choices\":[{\"index\":0,\"delta\":{\"tool_calls\":[{\"index\":0,\"function\":{\"arguments\":\"1}\"}},{\"index\":1,\"function\":{\"arguments\":\"2}\"}}]},\"finish_reason\":\"tool_calls\"}],\"usage\":{\"prompt_tokens\":3,\"completion_tokens\":4}}\n\n",
            "data: [DONE]\n\n"
        );
        let output = convert(ApiFormat::Messages, ApiFormat::ChatCompletions, source);
        assert!(output.contains("thinking_delta"));
        assert!(output.contains("你好"));
        assert_eq!(output.matches("tool_use\"").count(), 3); // two starts + stop reason
        assert!(output.contains("\"output_tokens\":4"));
        assert!(output.contains("message_stop"));

        let mut open_non_tool = None;
        for frame in output.split("\n\n") {
            let Some(payload) = frame.lines().find_map(|line| line.strip_prefix("data: ")) else {
                continue;
            };
            let Ok(value) = serde_json::from_str::<Value>(payload) else {
                continue;
            };
            match value.get("type").and_then(Value::as_str) {
                Some("content_block_start") => {
                    let kind = value
                        .pointer("/content_block/type")
                        .and_then(Value::as_str)
                        .unwrap_or("");
                    if matches!(kind, "thinking" | "text") {
                        assert!(open_non_tool.is_none(), "non-tool blocks must not overlap");
                        open_non_tool = value.get("index").and_then(Value::as_u64);
                    } else if kind == "tool_use" {
                        assert!(
                            open_non_tool.is_none(),
                            "thinking/text must close before a tool block"
                        );
                    }
                }
                Some("content_block_stop")
                    if value.get("index").and_then(Value::as_u64) == open_non_tool =>
                {
                    open_non_tool = None;
                }
                _ => {}
            }
        }
        assert!(open_non_tool.is_none());
    }

    #[test]
    fn messages_to_chat_translates_tools_usage_and_done() {
        let source = concat!(
            "event: message_start\ndata: {\"type\":\"message_start\",\"message\":{\"id\":\"msg_1\",\"model\":\"m\",\"usage\":{\"input_tokens\":5}}}\n\n",
            "event: content_block_start\ndata: {\"type\":\"content_block_start\",\"index\":0,\"content_block\":{\"type\":\"tool_use\",\"id\":\"t1\",\"name\":\"read\",\"input\":{}}}\n\n",
            "event: content_block_delta\ndata: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"input_json_delta\",\"partial_json\":\"{}\"}}\n\n",
            "event: content_block_stop\ndata: {\"type\":\"content_block_stop\",\"index\":0}\n\n",
            "event: message_delta\ndata: {\"type\":\"message_delta\",\"delta\":{\"stop_reason\":\"tool_use\"},\"usage\":{\"output_tokens\":2}}\n\n",
            "event: message_stop\ndata: {\"type\":\"message_stop\"}\n\n"
        );
        let output = convert(ApiFormat::ChatCompletions, ApiFormat::Messages, source);
        assert!(output.contains("tool_calls"));
        assert!(output.contains("\"completion_tokens\":2"));
        assert!(output.ends_with("data: [DONE]\n\n"));
    }

    #[test]
    fn responses_can_feed_both_other_protocols() {
        let source = concat!(
            "event: response.created\ndata: {\"type\":\"response.created\",\"response\":{\"id\":\"resp_1\",\"model\":\"m\",\"status\":\"in_progress\"}}\n\n",
            "event: response.output_text.delta\ndata: {\"type\":\"response.output_text.delta\",\"output_index\":0,\"content_index\":0,\"delta\":\"好\"}\n\n",
            "event: response.completed\ndata: {\"type\":\"response.completed\",\"response\":{\"status\":\"completed\",\"usage\":{\"input_tokens\":7,\"output_tokens\":1}}}\n\n"
        );
        let messages = convert(ApiFormat::Messages, ApiFormat::Responses, source);
        let chat = convert(ApiFormat::ChatCompletions, ApiFormat::Responses, source);
        assert!(messages.contains("text_delta"));
        assert!(messages.contains("message_stop"));
        assert!(chat.contains("\"content\":\"好\""));
        assert!(chat.ends_with("data: [DONE]\n\n"));
    }

    #[test]
    fn both_other_protocols_can_feed_responses() {
        let chat = concat!(
            "data: {\"id\":\"c\",\"model\":\"m\",\"choices\":[{\"delta\":{\"content\":\"好\"},\"finish_reason\":\"stop\"}]}\n\n",
            "data: [DONE]\n\n"
        );
        let messages = concat!(
            "event: message_start\ndata: {\"type\":\"message_start\",\"message\":{\"id\":\"msg_1\",\"model\":\"m\",\"usage\":{\"input_tokens\":1}}}\n\n",
            "event: content_block_start\ndata: {\"type\":\"content_block_start\",\"index\":0,\"content_block\":{\"type\":\"text\",\"text\":\"\"}}\n\n",
            "event: content_block_delta\ndata: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"text_delta\",\"text\":\"ok\"}}\n\n",
            "event: content_block_stop\ndata: {\"type\":\"content_block_stop\",\"index\":0}\n\n",
            "event: message_delta\ndata: {\"type\":\"message_delta\",\"delta\":{\"stop_reason\":\"end_turn\"},\"usage\":{\"output_tokens\":1}}\n\n",
            "event: message_stop\ndata: {\"type\":\"message_stop\"}\n\n"
        );
        for output in [
            convert(ApiFormat::Responses, ApiFormat::ChatCompletions, chat),
            convert(ApiFormat::Responses, ApiFormat::Messages, messages),
        ] {
            assert!(output.contains("response.output_item.added"));
            assert!(output.contains("response.output_text.delta"));
            assert!(output.contains("response.completed"));
            let timestamps = output
                .split("\n\n")
                .filter(|frame| {
                    frame.starts_with("event: response.created")
                        || frame.starts_with("event: response.completed")
                })
                .filter_map(|frame| frame.lines().find_map(|line| line.strip_prefix("data: ")))
                .map(|payload| {
                    serde_json::from_str::<Value>(payload).unwrap()["response"]["created_at"]
                        .as_u64()
                        .unwrap()
                })
                .collect::<Vec<_>>();
            assert_eq!(timestamps.len(), 2);
            assert!(timestamps[0] > 0);
            assert_eq!(timestamps[0], timestamps[1]);
        }
    }

    #[test]
    fn responses_tool_arguments_done_includes_function_name() {
        let source = concat!(
            "data: {\"id\":\"c\",\"model\":\"m\",\"choices\":[{\"delta\":{\"tool_calls\":[{\"index\":0,\"id\":\"call_1\",\"function\":{\"name\":\"read\",\"arguments\":\"{}\"}}]},\"finish_reason\":\"tool_calls\"}]}\n\n",
            "data: [DONE]\n\n"
        );
        let output = convert(ApiFormat::Responses, ApiFormat::ChatCompletions, source);
        let frame = output
            .split("\n\n")
            .find(|frame| frame.contains("response.function_call_arguments.done"))
            .expect("arguments done event");
        let payload = frame
            .lines()
            .find_map(|line| line.strip_prefix("data: "))
            .expect("event data");
        let value: Value = serde_json::from_str(payload).expect("valid event JSON");
        assert_eq!(value["name"], "read");
    }

    #[test]
    fn responses_restores_custom_tool_call_shape() {
        let mut custom_plan = plan(ApiFormat::Responses, ApiFormat::Messages);
        custom_plan.custom_tools = vec!["apply_patch".to_string()];
        custom_plan.response_parallel_tool_calls = false;
        custom_plan.response_tool_choice = json!("required");
        custom_plan.response_tools = vec![json!({"type":"custom","name":"apply_patch"})];
        let mut converter = StreamConverter::new(&custom_plan);
        let source = concat!(
            "event: message_start\ndata: {\"type\":\"message_start\",\"message\":{\"id\":\"msg_1\",\"model\":\"m\"}}\n\n",
            "event: content_block_start\ndata: {\"type\":\"content_block_start\",\"index\":0,\"content_block\":{\"type\":\"tool_use\",\"id\":\"call_1\",\"name\":\"apply_patch\",\"input\":{}}}\n\n",
            "event: content_block_delta\ndata: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"input_json_delta\",\"partial_json\":\"{\\\"input\\\":\\\"*** Begin\"}}\n\n",
            "event: content_block_delta\ndata: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"input_json_delta\",\"partial_json\":\" Patch\\\"}\"}}\n\n",
            "event: content_block_stop\ndata: {\"type\":\"content_block_stop\",\"index\":0}\n\n",
            "event: message_delta\ndata: {\"type\":\"message_delta\",\"delta\":{\"stop_reason\":\"tool_use\"},\"usage\":{\"output_tokens\":1}}\n\n",
            "event: message_stop\ndata: {\"type\":\"message_stop\"}\n\n"
        );
        let mut output = converter.process_chunk(Bytes::from(source)).unwrap();
        output.extend(converter.finish().unwrap());
        let output = String::from_utf8(output.concat()).unwrap();
        assert!(output.contains("\"type\":\"custom_tool_call\""));
        assert!(output.contains("\"name\":\"apply_patch\""));
        assert!(output.contains("\"input\":\"*** Begin Patch\""));
        assert!(!output.contains("response.function_call_arguments.delta"));
        let created = output
            .split("\n\n")
            .find(|frame| frame.starts_with("event: response.created"))
            .and_then(|frame| frame.lines().find_map(|line| line.strip_prefix("data: ")))
            .map(|payload| serde_json::from_str::<Value>(payload).unwrap())
            .unwrap();
        assert_eq!(created["response"]["parallel_tool_calls"], false);
        assert_eq!(created["response"]["tool_choice"], "required");
        assert_eq!(created["response"]["tools"][0]["name"], "apply_patch");
        let deltas = output
            .split("\n\n")
            .filter(|frame| frame.contains("response.custom_tool_call_input.delta"))
            .filter_map(|frame| frame.lines().find_map(|line| line.strip_prefix("data: ")))
            .map(|payload| serde_json::from_str::<Value>(payload).unwrap()["delta"].clone())
            .collect::<Vec<_>>();
        assert_eq!(deltas, [json!("*** Begin"), json!(" Patch")]);
    }

    #[test]
    fn responses_restores_namespace_tool_identity() {
        let mut namespace_plan = plan(ApiFormat::Responses, ApiFormat::Messages);
        namespace_plan.namespace_tools = vec![NamespaceToolMapping {
            flattened: "multi_agent_v1__spawn_agent".to_string(),
            namespace: "multi_agent_v1".to_string(),
            name: "spawn_agent".to_string(),
            custom: false,
        }];
        let mut converter = StreamConverter::new(&namespace_plan);
        let source = concat!(
            "event: message_start\ndata: {\"type\":\"message_start\",\"message\":{\"id\":\"msg_1\",\"model\":\"m\"}}\n\n",
            "event: content_block_start\ndata: {\"type\":\"content_block_start\",\"index\":0,\"content_block\":{\"type\":\"tool_use\",\"id\":\"call_1\",\"name\":\"multi_agent_v1__spawn_agent\",\"input\":{}}}\n\n",
            "event: content_block_delta\ndata: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"input_json_delta\",\"partial_json\":\"{}\"}}\n\n",
            "event: content_block_stop\ndata: {\"type\":\"content_block_stop\",\"index\":0}\n\n",
            "event: message_delta\ndata: {\"type\":\"message_delta\",\"delta\":{\"stop_reason\":\"tool_use\"},\"usage\":{\"output_tokens\":1}}\n\n",
            "event: message_stop\ndata: {\"type\":\"message_stop\"}\n\n"
        );
        let mut output = converter.process_chunk(Bytes::from(source)).unwrap();
        output.extend(converter.finish().unwrap());
        let output = String::from_utf8(output.concat()).unwrap();
        assert!(output.contains("\"namespace\":\"multi_agent_v1\""));
        assert!(output.contains("\"name\":\"spawn_agent\""));
        assert!(!output.contains("\"name\":\"multi_agent_v1__spawn_agent\""));
    }

    #[test]
    fn streaming_usage_normalizes_cached_tokens_for_each_target() {
        let messages = concat!(
            "event: message_start\ndata: {\"type\":\"message_start\",\"message\":{\"id\":\"msg_1\",\"model\":\"m\",\"usage\":{\"input_tokens\":6,\"cache_read_input_tokens\":4,\"cache_creation_input_tokens\":2}}}\n\n",
            "event: message_delta\ndata: {\"type\":\"message_delta\",\"delta\":{\"stop_reason\":\"end_turn\"},\"usage\":{\"output_tokens\":3}}\n\n",
            "event: message_stop\ndata: {\"type\":\"message_stop\"}\n\n"
        );
        let chat = convert(ApiFormat::ChatCompletions, ApiFormat::Messages, messages);
        assert!(chat.contains("\"prompt_tokens\":12"));
        assert!(chat.contains("\"cached_tokens\":4"));

        let chat_source = concat!(
            "data: {\"id\":\"c\",\"model\":\"m\",\"choices\":[{\"delta\":{},\"finish_reason\":\"stop\"}],\"usage\":{\"prompt_tokens\":12,\"completion_tokens\":3,\"prompt_tokens_details\":{\"cached_tokens\":4}}}\n\n",
            "data: [DONE]\n\n"
        );
        let anthropic = convert(ApiFormat::Messages, ApiFormat::ChatCompletions, chat_source);
        assert!(anthropic.contains("\"input_tokens\":8"));
        assert!(anthropic.contains("\"cache_read_input_tokens\":4"));
    }

    #[test]
    fn chat_finish_uses_last_anthropic_message_delta_usage() {
        let source = concat!(
            "event: message_start\ndata: {\"type\":\"message_start\",\"message\":{\"id\":\"msg_1\",\"model\":\"m\",\"usage\":{\"input_tokens\":2}}}\n\n",
            "event: message_delta\ndata: {\"type\":\"message_delta\",\"delta\":{\"stop_reason\":\"end_turn\"},\"usage\":{\"output_tokens\":1}}\n\n",
            "event: message_delta\ndata: {\"type\":\"message_delta\",\"delta\":{\"stop_reason\":\"end_turn\"},\"usage\":{\"output_tokens\":7}}\n\n",
            "event: message_stop\ndata: {\"type\":\"message_stop\"}\n\n"
        );
        let output = convert(ApiFormat::ChatCompletions, ApiFormat::Messages, source);
        assert!(output.contains("\"completion_tokens\":7"));
        assert_eq!(output.matches("\"finish_reason\":\"stop\"").count(), 1);
        assert!(output.ends_with("data: [DONE]\n\n"));
    }

    #[test]
    fn responses_errors_and_incomplete_stops_use_codex_events() {
        let converter =
            StreamConverter::new(&plan(ApiFormat::Responses, ApiFormat::ChatCompletions));
        let error = String::from_utf8(converter.error_event("boom").concat()).unwrap();
        assert!(error.contains("event: response.failed"));
        assert!(error.contains("\"status\":\"failed\""));
        assert!(error.contains("\"message\":\"boom\""));

        let refusal = concat!(
            "event: message_start\ndata: {\"type\":\"message_start\",\"message\":{\"id\":\"msg_1\",\"model\":\"m\",\"usage\":{\"input_tokens\":1}}}\n\n",
            "event: message_delta\ndata: {\"type\":\"message_delta\",\"delta\":{\"stop_reason\":\"refusal\"},\"usage\":{\"output_tokens\":1}}\n\n",
            "event: message_stop\ndata: {\"type\":\"message_stop\"}\n\n"
        );
        let output = convert(ApiFormat::Responses, ApiFormat::Messages, refusal);
        assert!(output.contains("event: response.incomplete"));
        assert!(output.contains("\"reason\":\"content_filter\""));
    }

    #[test]
    fn messages_signed_thinking_is_preserved_for_responses_replay() {
        let source = concat!(
            "event: message_start\ndata: {\"type\":\"message_start\",\"message\":{\"id\":\"msg_1\",\"model\":\"m\",\"usage\":{\"input_tokens\":1}}}\n\n",
            "event: content_block_start\ndata: {\"type\":\"content_block_start\",\"index\":0,\"content_block\":{\"type\":\"thinking\",\"thinking\":\"\"}}\n\n",
            "event: content_block_delta\ndata: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"thinking_delta\",\"thinking\":\"check\"}}\n\n",
            "event: content_block_delta\ndata: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"signature_delta\",\"signature\":\"sig_123\"}}\n\n",
            "event: content_block_stop\ndata: {\"type\":\"content_block_stop\",\"index\":0}\n\n",
            "event: message_delta\ndata: {\"type\":\"message_delta\",\"delta\":{\"stop_reason\":\"end_turn\"},\"usage\":{\"output_tokens\":1}}\n\n",
            "event: message_stop\ndata: {\"type\":\"message_stop\"}\n\n"
        );
        let output = convert(ApiFormat::Responses, ApiFormat::Messages, source);
        let frame = output
            .split("\n\n")
            .find(|frame| {
                frame.contains("response.output_item.done")
                    && frame.contains("\"type\":\"reasoning\"")
            })
            .expect("reasoning output item");
        let payload = frame
            .lines()
            .find_map(|line| line.strip_prefix("data: "))
            .expect("event data");
        let value: Value = serde_json::from_str(payload).expect("valid event JSON");
        let restored = super::super::protocol::decode_anthropic_thinking_block(
            value["item"]["encrypted_content"].as_str().unwrap(),
        )
        .expect("signed block decodes");
        assert_eq!(restored["thinking"], "check");
        assert_eq!(restored["signature"], "sig_123");
    }

    #[test]
    fn chat_reasoning_is_preserved_for_responses_replay() {
        let source = concat!(
            "data: {\"id\":\"c\",\"model\":\"m\",\"choices\":[{\"delta\":{\"reasoning_content\":\"reason\"},\"finish_reason\":null}]}\n\n",
            "data: {\"choices\":[{\"delta\":{\"tool_calls\":[{\"index\":0,\"id\":\"call_1\",\"function\":{\"name\":\"read\",\"arguments\":\"{}\"}}]},\"finish_reason\":\"tool_calls\"}]}\n\n",
            "data: [DONE]\n\n"
        );
        let output = convert(ApiFormat::Responses, ApiFormat::ChatCompletions, source);
        let frame = output
            .split("\n\n")
            .find(|frame| {
                frame.contains("response.output_item.done")
                    && frame.contains("\"type\":\"reasoning\"")
            })
            .expect("reasoning output item");
        let payload = frame
            .lines()
            .find_map(|line| line.strip_prefix("data: "))
            .expect("event data");
        let value: Value = serde_json::from_str(payload).expect("valid event JSON");
        let restored = super::super::protocol::decode_chat_reasoning(
            value["item"]["encrypted_content"].as_str().unwrap(),
        )
        .expect("chat reasoning decodes");
        assert_eq!(restored, "reason");
    }

    #[test]
    fn truncated_stream_is_not_synthesized_as_success() {
        let mut converter =
            StreamConverter::new(&plan(ApiFormat::Responses, ApiFormat::ChatCompletions));
        converter
            .process_chunk(Bytes::from_static(
                b"data: {\"id\":\"c\",\"choices\":[{\"delta\":{\"content\":\"partial\"},\"finish_reason\":null}]}\n\n",
            ))
            .expect("partial event converts");
        let error = converter.finish().expect_err("truncated stream must fail");
        assert!(error.message.contains("terminal event"));
    }
}

local transfem = "local meowing transfem catgirl club"

local function yield(value)
	coroutine.yield({ name = value, data = value, subtitle = "Hai fancy subtitle :3" })
end

return {
	actions = {
		luma.action.default("Default Action", ""),
		luma.action.suggest("Suggest Action", ""),
		luma.action.new("New Action", "", "Ctrl + Enter"),
		luma.action.new("New Action 2", "", { "Ctrl", "Alt", "Backspace" }),
	},
	config = {
		show_redi = { type = "checkbox", label = "Show `redi` option" },
		show_wiru = { type = "checkbox", label = "Show `wiru` option" },
		show_transfem = { type = "checkbox", label = "Show `local meowing transfem catgirl club` option" },
	},
	get_for_values = function(_, input, context)
		if context.config.show_redi and input:matches("redi") then
			yield("redi")
		end
		if context.config.show_wiru and input:matches("wiru") then
			yield("wiru")
		end
		if context.config.show_transfem and input:matches(transfem) then
			yield(transfem)
		end
	end,
	handle_pre = function(_, value, _)
		return luma.task.write_clipboard(value)
	end,
}

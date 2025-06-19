local values = {
	"redi",
	"wiru",
	"local meowing transfem catgirl club",
}

return {
	actions = {
		luma.action.default("Default Action", ""),
		luma.action.suggest("Suggest Action", ""),
		luma.action.new("New Action", "", "Ctrl + Enter"),
		luma.action.new("New Action 2", "", { "Ctrl", "Alt", "Backspace" }),
	},
	get_for_values = function(self, input)
		for _, value in ipairs(values) do
			if input:matches(value) then
				coroutine.yield({
					name = value,
					data = value,
					subtitle = "Hai fancy subtitle :3",
				})
			end
		end
	end,
	handle_pre = function(self, value, action)
		return luma.task.write_clipboard(value)
	end,
}

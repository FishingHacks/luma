local transfem = "local meowing transfem catgirl club"

local function yield(value)
	coroutine.yield({ name = value, data = value, subtitle = "Hai fancy subtitle :3" })
end

local opts = {}
for i = 1, 100, 1 do
	opts[i] = "Value " .. i
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
		show_wiru = { type = "toggle", label = "Show `wiru` option" },
		show_transfem = { type = "checkbox", label = "Show `local meowing transfem catgirl club` option" },

		test1 = "paragraph",
		test2 = "input",
		test3 = "int_input",
		test4 = "num_input",
		test5 = { type = "int_slider", min = 12, max = 120, step = 4 },
		test6 = { type = "slider", min = 0.2, max = 15, step = 0.5 },
		test7 = { type = "dropdown", values = { "Value 1", "Value 2", "Value 3", "Default Value" }, default = "Default Value" },
		test8 = { type = "searchable_dropdown", values = opts, default = "Value 67" },

		test9 = { type = "list", value_type = { type = "section", meow1 = "input", meow2 = { type = "searchable_dropdown", values = opts, default = "Value 69" } } },

		test10 = {
			type = "section",
			a = "input",
			b = "input",
			c = "input",
		},
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

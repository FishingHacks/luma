#if defined(__cplusplus)
#include "./cppapi.hpp"
#else

#include <stdint.h>
#include <stddef.h>
#include <stdlib.h>
#include <dirent.h>
#include <string.h>

typedef uint8_t bool;
#define true 1
#define false 0
typedef void* CustomData;

typedef struct String {
    char* data;
    size_t len;
} String;

#define STR(x) (String) { .data = x, .len = sizeof(x) }
typedef void MatcherInput;
typedef void ResultBuilder;

typedef struct Entry {
    CustomData data;
    String name;
    String subtitle;
} Entry;

#define NULL_ENTRY (Entry){ .data = 0 };

typedef struct Iterator {
    void* data;
    Entry (*next)(void*);
} Iterator;

typedef void* Task;

extern String home_dir();
extern Task Task_none();
extern Task Task_copy_to_clipboard(String s);
extern Task Task_chain(Task a, Task b);
extern Task Task_destroy(Task task);
extern bool MatcherInput_matches(const MatcherInput* const self, const String pattern);
extern String MatcherInput_string(const MatcherInput* const self);
extern bool ResultBuilder_commit(const ResultBuilder* const self, const Entry* const entries, size_t len);
extern bool ResultBuilder_commit_iter(const ResultBuilder* const self, Iterator iter);
extern CustomData allocate_customdata(size_t size);
extern void free_customdata(CustomData data);
extern String allocate_string(size_t len);
extern void free_string(String s);
extern void run(String cmd, String* args, size_t args_len);
extern void logs(String s);
extern void logc(char* s);
extern void logi32(int32_t i);
extern void logi64(int64_t l);
extern void logbool(bool b);
extern void logu32(uint32_t i);
extern void logu64(uint64_t l);
extern void elog(String s);
extern void elogc(char* s);
extern void elogi32(int32_t i);
extern void elogi64(int64_t l);
extern void elogbool(bool b);
extern void elogu32(uint32_t i);
extern void elogu64(uint64_t l);

String copy_string(String s) {
    String new = allocate_string(s.len);
    if(new.data == 0) return new;
    memcpy(new.data, s.data, s.len);
    new.len = s.len;
    return new;
}

typedef struct BoxDynPlugin {
    void (*init)(void*);
    String prefix;
    Task (*handle)(void*, CustomData);
    bool should_close;
    void (*filter)(void*, const MatcherInput* const, const ResultBuilder* const);
    void* (*create)();
    bool wants_thread;
} Plugin;





typedef struct Color {
    String name;
    String hex;
} Color;

typedef struct ColorIter {
    Color* arr;
    size_t len;
    const MatcherInput* const input;
} ColorIter;

Entry color_iter_next(ColorIter* self) {
    while(true) {
        if(self->len == 0) return NULL_ENTRY;
        self->len--;
        Color col = self->arr[0];
        self->arr = &self->arr[1];
        if(!MatcherInput_matches(self->input, col.name)) continue;

        CustomData data = allocate_customdata(sizeof(String));
        if (data == NULL) return NULL_ENTRY;
        *(String*)data = col.hex;
        String title = copy_string(col.name);
        String description = copy_string(col.hex);
        return (Entry){
            .name = title,
            .subtitle = description,
            .data = data,
        };
    }
}

Color colors[] = {
    { .name = STR("red"), .hex = STR("#ff0000") },
    { .name = STR("green"), .hex = STR("#00ff00") },
    { .name = STR("blue"), .hex = STR("#0000ff") },
    { .name = STR("yellow"), .hex = STR("#0000ff") },
    { .name = STR("pink"), .hex = STR("#ff00ff") },
};

typedef void HomeDirPlugin;

HomeDirPlugin* create() {
    return NULL;
}

Task handle(HomeDirPlugin* plugin, CustomData data) {
    run(STR("xdg-open"), (String*)data, 1);
    return Task_none();
}

void filter(HomeDirPlugin* plugin, const MatcherInput* const input, const ResultBuilder* const builder) {
    ColorIter iter = {
        .arr = (Color*)&colors,
        .len = sizeof(colors),
        .input = input,
    };
    ResultBuilder_commit_iter(builder, (Iterator){
        .next = (Entry(*)(void*))&color_iter_next,
        .data = &iter,
    });
}

Plugin plugin() {
    return (Plugin) {
        .prefix = STR("~"),
        .should_close = true,
        .create = &create,
        .handle = &handle,
        .init = NULL,
        .filter = &filter,
        .wants_thread = false,
    };
}
#endif

#include <cstdint>
#include <cstring>
#include <optional>

typedef void* CustomData;

struct String {
    char* data;
    size_t len;
};

extern bool MatcherInput_matches(const void* const self, const String pattern);
extern String MatcherInput_string(const void* const self);

class MatcherInput {
    MatcherInput() = delete;
    ~MatcherInput() = delete;

    bool matches(String pattern) {
        return MatcherInput_matches(this, pattern);
    }
    String string() {
        return MatcherInput_string(this);
    }
};
typedef void ResultBuilder;

typedef struct Entry {
    CustomData data;
    String name;
    String subtitle;
} Entry;

#define NULL_ENTRY (Entry){ .data = 0 };

struct DynIterator {
    void* data;
    Entry (*next)(void*);
};

template<typename T, typename F>
class FilterIter;
template<typename O, typename N, typename F>
class MapIter;
template<typename T>
class OnceIter;

template<typename T>
class Iterator {
public:
    virtual std::optional<T> next();

    template<typename F>
    FilterIter<T, F> filter(F filter) {
        FilterIter<T, F> iter(*this, filter);
        return iter;
    }
    template<typename N, typename F>
    MapIter<N, T, F> map(F filter) {
        MapIter<N, T, F> iter(*this, filter);
        return iter;
    }

    static OnceIter<T> once(T item) {
        return(item);
    }
};

template<typename O, typename N, typename F>
class MapIter : public Iterator<N> {
    Iterator<O> inner;
    F map_fn;

public:
    std::optional<N> next() {
        auto value = inner.next();
        if (value.has_value()) return map_fn(value.value());
        return std::nullopt;
    }

    MapIter(Iterator<O> inner, F map) : map_fn(map), inner(inner) {}
};

template<typename T>
class OnceIter : public Iterator<T> {
    std::optional<T> item;
public:
    OnceIter(T item) : item(item) {}

    std::optional<T> next() {
        std::optional<T> v;
        v.swap(item);
        return v;
    }
};

template<typename T, typename F>
class FilterIter : public Iterator<T> {
    Iterator<T> inner;
    F filter_fn;

public:
    std::optional<T> next() {
        while(auto value = inner.next()) {
            if(filter_fn(value.value())) return value;
        }
        return std::nullopt;
    }
    FilterIter(Iterator<T> inner, F filter) : filter_fn(filter), inner(inner) {}
};

Entry __make_dyn_next(Iterator<Entry>* iter) {
    auto next_val = iter->next();
    if(next_val.has_value()) return next_val.value();
    return NULL_ENTRY;
}

DynIterator make_dyn(Iterator<Entry>* iter) {
    return {
        .data = iter,
        .next = (Entry(*)(void*))&__make_dyn_next,
    };
}

class Task;

extern Task Task_none();
extern Task Task_copy_to_clipboard(String s);
extern Task Task_chain(void* a, void* b);
extern void Task_destroy(void* task);
extern String allocate_string(size_t len);
extern void free_string(String s);

String copy_string(String s) {
    String new_s = allocate_string(s.len);
    if(new_s.data == 0) return new_s;
    memcpy(new_s.data, s.data, s.len);
    new_s.data = s.data;
    return new_s;
}

class Task {
    Task() = delete;
    ~Task() {
        if (inner) Task_destroy(inner);
    }
    void* inner;
public:
    Task chain(Task other) {
        void* me = inner;
        void* other_inner = other.inner;
        inner = nullptr;
        other.inner = nullptr;
        return Task_chain(me, other_inner);
    }

    static Task none() {
        return Task_none();
    }
    static Task copy_to_clipboard(String s) {
        return Task_copy_to_clipboard(s);
    }
};

extern String home_dir();
extern bool ResultBuilder_commit(const ResultBuilder* const self, const Entry* const entries, size_t len);
extern bool ResultBuilder_commit_iter(const ResultBuilder* const self, DynIterator iter);
extern CustomData allocate_customdata(size_t size);
extern void free_customdata(CustomData data);
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

typedef struct BoxDynPlugin {
    void (*init)(void*);
    String prefix;
    Task (*handle)(void*, CustomData);
    bool should_close;
    void (*filter)(void*, const MatcherInput* const, const ResultBuilder* const);
    void* (*create)();
    bool wants_thread;
} Plugin;

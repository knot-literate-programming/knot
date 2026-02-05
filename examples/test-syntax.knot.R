















#| eval: true
#| echo: true
library(dplyr)
library(ggplot2)
x <- 1:10
y <- x^2 + rnorm(10)
df <- data.frame(x = x, y = y)





# This should be formatted by Air
result <- df %>%
  filter(x > 5) %>%
  mutate(z = x + y) %>%
  summarize(mean_z = mean(z))





for (i in 1:10) {
  if (i %% 2 == 0) {
    print(i)
  } else {
    print("odd")
  }
}



















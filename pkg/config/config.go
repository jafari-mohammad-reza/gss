package config

import (
	"errors"
	"fmt"
	"strings"

	"github.com/go-playground/validator/v10"
	"github.com/spf13/viper"
)

type Config struct {
	ServerPort int    `mapstructure:"port"`
	ServerHost string `mapstructure:"host"`
	DB         int    `mapstructure:"db"`
	AOF        bool   `mapstructure:"aof"`
	AOFFile    string `mapstructure:"aof_file"`
}

func NewConfig() (*Config, error) {
	v := viper.New()
	v.SetConfigName("config")
	v.SetConfigType("yaml")
	v.AddConfigPath(".")
	v.AddConfigPath("./config")
	if err := v.ReadInConfig(); err != nil {
		return nil, err
	}
	v.SetDefault("port", 8082)
	v.SetDefault("host", "0.0.0.0")
	v.SetDefault("db", 0)
	v.SetDefault("aof", true)
	v.SetDefault("aof_file", "appendonly.aof")
	var config Config
	if err := v.Unmarshal(&config); err != nil {
		return nil, err
	}
	validate := validator.New()
	if err := validate.Struct(&config); err != nil {
		var sb strings.Builder
		for _, err := range err.(validator.ValidationErrors) {
			sb.WriteString(fmt.Sprintf("Field '%s' failed on '%s'\n", err.Field(), err.Tag()))
		}
		return nil, errors.New(sb.String())
	}
	return &config, nil
}

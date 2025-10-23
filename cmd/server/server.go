package main

import (
	"context"
	"log"
	"os/signal"
	"syscall"
	"time"

	"github.com/jafari-mohammad-reza/gss/internal/p2p"
	"github.com/jafari-mohammad-reza/gss/pkg/config"
)

func main() {
	ctx, cancel := signal.NotifyContext(context.Background(), syscall.SIGINT, syscall.SIGTERM)
	defer cancel()
	conf, err := config.NewConfig()
	if err != nil {
		log.Fatalf("failed to load config: %s", err.Error())
	}
	ser, err := p2p.NewServer(conf)
	if err != nil {
		panic(err)
	}
	if err := ser.Start(ctx); err != nil {
		panic(err)
	}
	<-ctx.Done()
	log.Printf("shutting down server")
	stopCtx, cancel := context.WithTimeout(context.Background(), time.Second*10)
	defer cancel()
	if err := ser.Stop(stopCtx); err != nil {
		log.Fatalf("failed to gracefully shutdown server: %s", err.Error())
	}
}
